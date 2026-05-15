//! 5 种 Locator + resolve(content) → 行号区间。
//!
//! 行的定义（PLAN.md §6.4 / §6.1）：
//! - 字节流按 LF (`\n`) 切分；CRLF 文件 `\r` 留在行内
//! - 行号 **1-based 闭区间**
//! - 末尾若不以 `\n` 结尾，仍算一个完整行
//! - 空文件 = 0 行
//!
//! `Pattern` 由 `ops::replace` 内部用 regex 直接处理（不解析为行号区间），
//! 因此 `resolve` 对 `Pattern` 返回 `LocateError::PatternNotResolvable`。

use regex::Regex;
use regex::bytes::Regex as ByteRegex;
use thiserror::Error;

/// 字面字节序列；保留二进制透明。
pub type ByteSeq = Vec<u8>;

/// 命中计数（仅 `--pattern` 模式使用）。
#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
pub enum Count {
    /// 仅替换前 N 处。
    First(usize),
    /// 全部替换。
    #[default]
    All,
}

/// 5 种定位维度。写命令必须且仅有一个。
#[derive(Debug)]
pub enum Locator {
    /// `--lines a[-b]`，1-based 闭区间。
    Lines { start: usize, end: usize },
    /// `--match-line <regex>` + `--occurrence N`（默认 1）。
    MatchLine { regex: Regex, occurrence: usize },
    /// `--between <start>..<end>`；字面锚点（行级 `contains`）。
    Between {
        start: ByteSeq,
        end: ByteSeq,
        start_occ: usize,
        end_occ: usize,
        inclusive: bool,
    },
    /// `--between-re <re>..<re>`；正则锚点（行级匹配）。
    BetweenRe {
        start: Regex,
        end: Regex,
        start_occ: usize,
        end_occ: usize,
        inclusive: bool,
    },
    /// `--pattern <regex>`，仅 `replace` 子模式；不通过 resolve 解析。
    Pattern { regex: ByteRegex, count: Count },
}

/// 解析后的行号区间，1-based 闭区间。
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Resolved {
    pub start_line: usize,
    pub end_line: usize,
}

/// 定位失败的原因。
#[derive(Debug, Error)]
pub enum LocateError {
    #[error("line range out of bounds: requested {requested:?}, file has {actual} lines")]
    OutOfBounds {
        requested: (usize, usize),
        actual: usize,
    },
    #[error("invalid range: end ({end}) < start ({start})")]
    InvalidRange { start: usize, end: usize },
    #[error("zero is not a valid 1-based line number")]
    ZeroLine,
    #[error("--match-line regex did not match (occurrence {occurrence})")]
    MatchLineNotFound { occurrence: usize },
    #[error("--between start anchor not found (occurrence {occurrence})")]
    BetweenStartNotFound { occurrence: usize },
    #[error("--between end anchor not found after start (occurrence {occurrence})")]
    BetweenEndNotFound { occurrence: usize },
    #[error("--pattern locator must be resolved by ops::replace, not by resolve()")]
    PatternNotResolvable,
}

/// 把字节流按 `\n` 切分为行视图（不包含分隔符 `\n`，但保留 `\r`）。
///
/// 末尾不以 `\n` 结尾时，最后一行仍计入。空文件返回空 vec。
pub(crate) fn split_lines(content: &[u8]) -> Vec<&[u8]> {
    if content.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut start = 0usize;
    for (i, b) in content.iter().enumerate() {
        if *b == b'\n' {
            out.push(&content[start..i]);
            start = i + 1;
        }
    }
    // 末尾不以 \n 结尾 → 还有一行未推入
    if start < content.len() {
        out.push(&content[start..]);
    }
    out
}

/// 提取 `[start, end]`（1-based 闭区间）行的字节序列，**保留每行的尾随 `\n`**
/// （仅当原本就有；最后一行若不带 `\n` 则不补）。
///
/// 越界（`start == 0`、`end > total`、`start > end`）时返回空 `Vec`。
/// 主要用于 `--was` 校验、`--revert` 前置匹配等场景。
#[must_use]
pub fn extract_line_range_with_newline(content: &[u8], start: usize, end: usize) -> Vec<u8> {
    if start == 0 || end < start {
        return Vec::new();
    }
    let lines = split_lines(content);
    if end > lines.len() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let base = content.as_ptr() as usize;
    for line in lines.iter().take(end).skip(start - 1) {
        out.extend_from_slice(line);
        let line_start = line.as_ptr() as usize;
        let end_idx = line_start - base + line.len();
        if content.get(end_idx) == Some(&b'\n') {
            out.push(b'\n');
        }
    }
    out
}

/// 把 `Locator` 解析为具体行号区间（1-based 闭区间）。
///
/// `Pattern` 不通过此函数解析（由 `ops::replace` 直接用 regex 处理整个字节流）。
///
/// # Errors
/// 见 [`LocateError`]。
pub fn resolve(loc: &Locator, content: &[u8]) -> Result<Resolved, LocateError> {
    let lines = split_lines(content);
    let total = lines.len();

    match loc {
        Locator::Lines { start, end } => {
            if *start == 0 || *end == 0 {
                return Err(LocateError::ZeroLine);
            }
            if end < start {
                return Err(LocateError::InvalidRange {
                    start: *start,
                    end: *end,
                });
            }
            if *end > total {
                return Err(LocateError::OutOfBounds {
                    requested: (*start, *end),
                    actual: total,
                });
            }
            Ok(Resolved {
                start_line: *start,
                end_line: *end,
            })
        }
        Locator::MatchLine { regex, occurrence } => {
            if *occurrence == 0 {
                return Err(LocateError::ZeroLine);
            }
            let mut hit = 0usize;
            for (idx, line) in lines.iter().enumerate() {
                let s = String::from_utf8_lossy(line);
                if regex.is_match(&s) {
                    hit += 1;
                    if hit == *occurrence {
                        let n = idx + 1;
                        return Ok(Resolved {
                            start_line: n,
                            end_line: n,
                        });
                    }
                }
            }
            Err(LocateError::MatchLineNotFound {
                occurrence: *occurrence,
            })
        }
        Locator::Between {
            start,
            end,
            start_occ,
            end_occ,
            inclusive,
        } => resolve_between_with(
            &lines,
            *start_occ,
            *end_occ,
            *inclusive,
            |line| line.windows(start.len()).any(|w| w == start.as_slice()),
            |line| line.windows(end.len()).any(|w| w == end.as_slice()),
        ),
        Locator::BetweenRe {
            start,
            end,
            start_occ,
            end_occ,
            inclusive,
        } => resolve_between_with(
            &lines,
            *start_occ,
            *end_occ,
            *inclusive,
            |line| {
                let s = String::from_utf8_lossy(line);
                start.is_match(&s)
            },
            |line| {
                let s = String::from_utf8_lossy(line);
                end.is_match(&s)
            },
        ),
        Locator::Pattern { .. } => Err(LocateError::PatternNotResolvable),
    }
}

/// `between` 与 `between-re` 共用的扫描逻辑。
///
/// - 在前 N 个 start 命中中取第 `start_occ`（1-based）；从该行 **之后** 找第 `end_occ` 个 end 命中
/// - `inclusive == true` → 返回 `[start_line, end_line]`
/// - `inclusive == false` → 返回 `[start_line + 1, end_line - 1]`；
///   若结果区间为空（end 紧邻 start），返回 `[start_line + 1, start_line]` 这种倒序状态
///   会被 `Resolved` 调用方按 "空区间删除/插入" 语义处理；这里直接返回 `InvalidRange`
fn resolve_between_with<FStart, FEnd>(
    lines: &[&[u8]],
    start_occ: usize,
    end_occ: usize,
    inclusive: bool,
    is_start: FStart,
    is_end: FEnd,
) -> Result<Resolved, LocateError>
where
    FStart: Fn(&[u8]) -> bool,
    FEnd: Fn(&[u8]) -> bool,
{
    if start_occ == 0 || end_occ == 0 {
        return Err(LocateError::ZeroLine);
    }

    // find start
    let mut hit = 0usize;
    let mut start_idx: Option<usize> = None;
    for (i, line) in lines.iter().enumerate() {
        if is_start(line) {
            hit += 1;
            if hit == start_occ {
                start_idx = Some(i);
                break;
            }
        }
    }
    let s_idx = start_idx.ok_or(LocateError::BetweenStartNotFound {
        occurrence: start_occ,
    })?;

    // find end after start
    let mut hit = 0usize;
    let mut end_idx: Option<usize> = None;
    for (i, line) in lines.iter().enumerate().skip(s_idx + 1) {
        if is_end(line) {
            hit += 1;
            if hit == end_occ {
                end_idx = Some(i);
                break;
            }
        }
    }
    let e_idx = end_idx.ok_or(LocateError::BetweenEndNotFound {
        occurrence: end_occ,
    })?;

    let (start_line, end_line) = if inclusive {
        (s_idx + 1, e_idx + 1)
    } else {
        // exclude both anchors
        let s = s_idx + 2;
        let e = e_idx; // (e_idx + 1) - 1
        if s > e {
            return Err(LocateError::InvalidRange { start: s, end: e });
        }
        (s, e)
    };

    Ok(Resolved {
        start_line,
        end_line,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(c: &[u8]) -> Vec<String> {
        split_lines(c)
            .iter()
            .map(|l| String::from_utf8_lossy(l).into_owned())
            .collect()
    }

    #[test]
    fn split_lines_empty() {
        assert_eq!(split_lines(b"").len(), 0);
    }

    #[test]
    fn split_lines_no_trailing_newline() {
        assert_eq!(lines(b"a\nb"), vec!["a", "b"]);
    }

    #[test]
    fn split_lines_with_trailing_newline() {
        assert_eq!(lines(b"a\nb\n"), vec!["a", "b"]);
    }

    #[test]
    fn split_lines_preserves_cr() {
        assert_eq!(lines(b"a\r\nb\r\n"), vec!["a\r", "b\r"]);
    }

    #[test]
    fn split_lines_blank_line() {
        assert_eq!(lines(b"a\n\nb\n"), vec!["a", "", "b"]);
    }

    #[test]
    fn lines_simple_range() {
        let r = resolve(&Locator::Lines { start: 2, end: 3 }, b"a\nb\nc\nd\n").unwrap();
        assert_eq!(
            r,
            Resolved {
                start_line: 2,
                end_line: 3
            }
        );
    }

    #[test]
    fn lines_single_line() {
        let r = resolve(&Locator::Lines { start: 1, end: 1 }, b"only\n").unwrap();
        assert_eq!(
            r,
            Resolved {
                start_line: 1,
                end_line: 1
            }
        );
    }

    #[test]
    fn lines_zero_is_invalid() {
        let e = resolve(&Locator::Lines { start: 0, end: 1 }, b"a\n").unwrap_err();
        matches!(e, LocateError::ZeroLine);
    }

    #[test]
    fn lines_inverted_range_invalid() {
        let e = resolve(&Locator::Lines { start: 3, end: 2 }, b"a\nb\nc\n").unwrap_err();
        matches!(e, LocateError::InvalidRange { .. });
    }

    #[test]
    fn lines_out_of_bounds() {
        let e = resolve(&Locator::Lines { start: 1, end: 99 }, b"a\nb\n").unwrap_err();
        matches!(e, LocateError::OutOfBounds { .. });
    }

    #[test]
    fn match_line_first_occurrence() {
        let loc = Locator::MatchLine {
            regex: Regex::new(r"^foo").unwrap(),
            occurrence: 1,
        };
        let r = resolve(&loc, b"bar\nfoo\nbaz\nfoo\n").unwrap();
        assert_eq!(r.start_line, 2);
        assert_eq!(r.end_line, 2);
    }

    #[test]
    fn match_line_nth_occurrence() {
        let loc = Locator::MatchLine {
            regex: Regex::new(r"^foo").unwrap(),
            occurrence: 2,
        };
        let r = resolve(&loc, b"bar\nfoo\nbaz\nfoo\n").unwrap();
        assert_eq!(r.start_line, 4);
    }

    #[test]
    fn match_line_no_match() {
        let loc = Locator::MatchLine {
            regex: Regex::new(r"^xxx").unwrap(),
            occurrence: 1,
        };
        let e = resolve(&loc, b"a\nb\n").unwrap_err();
        matches!(e, LocateError::MatchLineNotFound { .. });
    }

    #[test]
    fn between_inclusive() {
        let loc = Locator::Between {
            start: b"BEGIN".to_vec(),
            end: b"END".to_vec(),
            start_occ: 1,
            end_occ: 1,
            inclusive: true,
        };
        let r = resolve(&loc, b"x\n// BEGIN\ninner\n// END\ny\n").unwrap();
        assert_eq!(r.start_line, 2);
        assert_eq!(r.end_line, 4);
    }

    #[test]
    fn between_exclusive() {
        let loc = Locator::Between {
            start: b"BEGIN".to_vec(),
            end: b"END".to_vec(),
            start_occ: 1,
            end_occ: 1,
            inclusive: false,
        };
        let r = resolve(&loc, b"x\n// BEGIN\ninner1\ninner2\n// END\ny\n").unwrap();
        assert_eq!(r.start_line, 3);
        assert_eq!(r.end_line, 4);
    }

    #[test]
    fn between_start_not_found() {
        let loc = Locator::Between {
            start: b"BEGIN".to_vec(),
            end: b"END".to_vec(),
            start_occ: 1,
            end_occ: 1,
            inclusive: true,
        };
        let e = resolve(&loc, b"x\ny\n").unwrap_err();
        matches!(e, LocateError::BetweenStartNotFound { .. });
    }

    #[test]
    fn between_end_not_found_after_start() {
        let loc = Locator::Between {
            start: b"BEGIN".to_vec(),
            end: b"END".to_vec(),
            start_occ: 1,
            end_occ: 1,
            inclusive: true,
        };
        let e = resolve(&loc, b"// BEGIN\ninner\n").unwrap_err();
        matches!(e, LocateError::BetweenEndNotFound { .. });
    }

    #[test]
    fn between_re_inclusive() {
        let loc = Locator::BetweenRe {
            start: Regex::new(r"^function foo").unwrap(),
            end: Regex::new(r"^\}").unwrap(),
            start_occ: 1,
            end_occ: 1,
            inclusive: true,
        };
        let body = b"x\nfunction foo() {\n  return 1;\n}\ny\n";
        let r = resolve(&loc, body).unwrap();
        assert_eq!(r.start_line, 2);
        assert_eq!(r.end_line, 4);
    }

    #[test]
    fn pattern_locator_not_resolvable() {
        let loc = Locator::Pattern {
            regex: ByteRegex::new(r"foo").unwrap(),
            count: Count::All,
        };
        let e = resolve(&loc, b"foo\n").unwrap_err();
        matches!(e, LocateError::PatternNotResolvable);
    }

    #[test]
    fn count_default_is_all() {
        assert_eq!(Count::default(), Count::All);
    }
}
