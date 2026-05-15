//! `peek`：输出带行号的指定区间。
//!
//! 三种模式（PLAN.md §6.7.1）：
//! - `--lines a[-b]`：直接行号
//! - `--match-line RE [--context N]`：匹配行 ± N 上下文
//! - `--all`：全文（默认 `--max-lines 1000`）
//!
//! 输出格式：
//! ```text
//!    30: const X = 1;
//!    31: function foo() {
//! ```
//! 行号右对齐 6 字符宽，冒号后单空格，内容字节透传。

use std::io::Write;

use regex::Regex;
use thiserror::Error;

use crate::core::location::{LocateError, Locator, Resolved, resolve, split_lines};

/// `peek` 选择哪个区间。
#[derive(Debug)]
pub enum PeekRange {
    /// `--lines a[-b]`。
    Lines { start: usize, end: usize },
    /// `--match-line RE [--context N]`。
    MatchLine { regex: Regex, context: usize },
    /// `--all`。
    All,
}

/// `peek` 的执行选项。
#[derive(Debug)]
pub struct PeekOpts {
    pub range: PeekRange,
    /// 全文与正则模式下的最大输出行数；超出截断。`None` 表示不限。
    pub max_lines: Option<usize>,
}

#[derive(Debug, Error)]
pub enum PeekError {
    #[error(transparent)]
    Locate(#[from] LocateError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// peek 执行结果，便于上层判断是否截断。
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct PeekResult {
    /// 实际输出的行数。
    pub lines_written: usize,
    /// 是否因 `max_lines` 截断。
    pub truncated: bool,
}

/// 执行 peek 并把结果写入 `out`。
///
/// # Errors
/// 见 [`PeekError`]。
pub fn run<W: Write>(content: &[u8], opts: &PeekOpts, mut out: W) -> Result<PeekResult, PeekError> {
    let lines = split_lines(content);
    let total = lines.len();

    // 计算需要输出的行号集合（1-based 闭区间序列）。
    let ranges: Vec<Resolved> = match &opts.range {
        PeekRange::Lines { start, end } => {
            let r = resolve(
                &Locator::Lines {
                    start: *start,
                    end: *end,
                },
                content,
            )?;
            vec![r]
        }
        PeekRange::MatchLine { regex, context } => {
            // 复用 location::resolve(MatchLine) 只能拿第一个；这里需要全部命中
            let mut hits: Vec<Resolved> = Vec::new();
            for (idx, line) in lines.iter().enumerate() {
                let s = String::from_utf8_lossy(line);
                if regex.is_match(&s) {
                    let n = idx + 1;
                    let lo = n.saturating_sub(*context).max(1);
                    let hi = (n + *context).min(total);
                    hits.push(Resolved {
                        start_line: lo,
                        end_line: hi,
                    });
                }
            }
            if hits.is_empty() {
                return Err(PeekError::Locate(LocateError::MatchLineNotFound {
                    occurrence: 1,
                }));
            }
            // 合并相邻/重叠区间，避免上下文重复
            merge_ranges(hits)
        }
        PeekRange::All => {
            if total == 0 {
                vec![]
            } else {
                vec![Resolved {
                    start_line: 1,
                    end_line: total,
                }]
            }
        }
    };

    // 按区间逐行输出，附带 max_lines 截断
    let cap = opts.max_lines.unwrap_or(usize::MAX);
    let mut written = 0usize;
    let mut truncated = false;

    'outer: for r in ranges {
        for n in r.start_line..=r.end_line {
            if written >= cap {
                truncated = true;
                break 'outer;
            }
            let line = lines.get(n - 1).copied().unwrap_or(b"");
            // 行号右对齐 6 字符 + ": " + 字节内容 + \n
            write!(out, "{n:>6}: ")?;
            out.write_all(line)?;
            out.write_all(b"\n")?;
            written += 1;
        }
    }

    Ok(PeekResult {
        lines_written: written,
        truncated,
    })
}

/// 合并已按起始行升序的若干区间（也允许乱序，函数内排序）。重叠或相邻则合并。
fn merge_ranges(mut rs: Vec<Resolved>) -> Vec<Resolved> {
    if rs.is_empty() {
        return rs;
    }
    rs.sort_by_key(|r| r.start_line);
    let mut merged: Vec<Resolved> = Vec::with_capacity(rs.len());
    let mut cur = rs[0];
    for next in rs.into_iter().skip(1) {
        if next.start_line <= cur.end_line + 1 {
            cur.end_line = cur.end_line.max(next.end_line);
        } else {
            merged.push(cur);
            cur = next;
        }
    }
    merged.push(cur);
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_str(content: &[u8], opts: &PeekOpts) -> (String, PeekResult) {
        let mut buf = Vec::new();
        let res = run(content, opts, &mut buf).unwrap();
        (String::from_utf8(buf).unwrap(), res)
    }

    #[test]
    fn lines_basic_format() {
        let (out, res) = run_str(
            b"a\nb\nc\nd\n",
            &PeekOpts {
                range: PeekRange::Lines { start: 2, end: 3 },
                max_lines: None,
            },
        );
        assert_eq!(out, "     2: b\n     3: c\n");
        assert_eq!(res.lines_written, 2);
        assert!(!res.truncated);
    }

    #[test]
    fn lines_single() {
        let (out, _) = run_str(
            b"first\nsecond\n",
            &PeekOpts {
                range: PeekRange::Lines { start: 1, end: 1 },
                max_lines: None,
            },
        );
        assert_eq!(out, "     1: first\n");
    }

    #[test]
    fn lines_out_of_bounds_yields_locate_err() {
        let mut buf = Vec::new();
        let err = run(
            b"a\n",
            &PeekOpts {
                range: PeekRange::Lines { start: 1, end: 5 },
                max_lines: None,
            },
            &mut buf,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            PeekError::Locate(LocateError::OutOfBounds { .. })
        ));
    }

    #[test]
    fn all_outputs_full_file() {
        let (out, res) = run_str(
            b"a\nb\nc\n",
            &PeekOpts {
                range: PeekRange::All,
                max_lines: None,
            },
        );
        assert_eq!(out, "     1: a\n     2: b\n     3: c\n");
        assert_eq!(res.lines_written, 3);
    }

    #[test]
    fn all_with_max_lines_truncates() {
        let (out, res) = run_str(
            b"a\nb\nc\nd\ne\n",
            &PeekOpts {
                range: PeekRange::All,
                max_lines: Some(2),
            },
        );
        assert_eq!(out, "     1: a\n     2: b\n");
        assert_eq!(res.lines_written, 2);
        assert!(res.truncated);
    }

    #[test]
    fn match_line_with_context() {
        let body = b"a\nfoo\nb\nc\n";
        let (out, _) = run_str(
            body,
            &PeekOpts {
                range: PeekRange::MatchLine {
                    regex: Regex::new(r"^foo").unwrap(),
                    context: 1,
                },
                max_lines: None,
            },
        );
        assert_eq!(out, "     1: a\n     2: foo\n     3: b\n");
    }

    #[test]
    fn match_line_multiple_hits_merge_overlap() {
        let body = b"a\nfoo\nb\nfoo\nc\n";
        let (out, _) = run_str(
            body,
            &PeekOpts {
                range: PeekRange::MatchLine {
                    regex: Regex::new(r"^foo").unwrap(),
                    context: 1,
                },
                max_lines: None,
            },
        );
        // hits at line 2 (1..=3) and line 4 (3..=5); merged to 1..=5
        assert_eq!(
            out,
            "     1: a\n     2: foo\n     3: b\n     4: foo\n     5: c\n"
        );
    }

    #[test]
    fn match_line_not_found_yields_locate_err() {
        let mut buf = Vec::new();
        let err = run(
            b"a\nb\n",
            &PeekOpts {
                range: PeekRange::MatchLine {
                    regex: Regex::new(r"^xxx").unwrap(),
                    context: 0,
                },
                max_lines: None,
            },
            &mut buf,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            PeekError::Locate(LocateError::MatchLineNotFound { .. })
        ));
    }

    #[test]
    fn bytes_passthrough_for_chinese() {
        let body = "1\n你好🌟\n3\n".as_bytes();
        let (out, _) = run_str(
            body,
            &PeekOpts {
                range: PeekRange::Lines { start: 2, end: 2 },
                max_lines: None,
            },
        );
        assert_eq!(out, "     2: 你好🌟\n");
    }

    #[test]
    fn empty_file_with_all_outputs_nothing() {
        let (out, res) = run_str(
            b"",
            &PeekOpts {
                range: PeekRange::All,
                max_lines: None,
            },
        );
        assert_eq!(out, "");
        assert_eq!(res.lines_written, 0);
    }
}
