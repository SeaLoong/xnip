//! `find`：搜索定位，输出 `path:line[:col]` 列表。
//!
//! 两种模式（PLAN.md §6.7.2）：
//! - `--match-line RE`：整行命中；输出 `path:line`（无 col）
//! - `--pattern RE`：跨行字节级匹配；输出 `path:line:col`（1-based byte col）
//!
//! 跨文件并发：M2 暂用顺序扫描；M4 加 `rayon`。

use std::io::Write;

use regex::Regex;
use regex::bytes::Regex as ByteRegex;
use thiserror::Error;

use crate::core::location::split_lines;

#[derive(Debug)]
pub enum FindMode<'a> {
    /// 整行命中正则；col 字段始终为 0。
    MatchLine(&'a Regex),
    /// 字节级跨行命中；col 是 1-based byte offset 在该行内。
    Pattern(&'a ByteRegex),
}

#[derive(Debug)]
pub struct FindOpts<'a> {
    pub mode: FindMode<'a>,
    /// 命中数上限（per-file 还是 total，取决于调用方语义；这里按 total 用）。`None` 不限。
    pub max_matches: Option<usize>,
    /// `true` 时每个文件命中第 1 处即停。
    pub first_only: bool,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Hit {
    pub line: usize,
    /// 1-based byte column within line；`MatchLine` 模式恒为 0。
    pub col: usize,
}

#[derive(Debug, Error)]
pub enum FindError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// 在单个文件字节流里搜索，返回所有命中。
pub fn scan(content: &[u8], opts: &FindOpts<'_>) -> Vec<Hit> {
    let lines = split_lines(content);
    let mut out = Vec::new();
    match opts.mode {
        FindMode::MatchLine(re) => {
            for (idx, line) in lines.iter().enumerate() {
                let s = String::from_utf8_lossy(line);
                if re.is_match(&s) {
                    out.push(Hit {
                        line: idx + 1,
                        col: 0,
                    });
                    if opts.first_only {
                        break;
                    }
                }
            }
        }
        FindMode::Pattern(re) => {
            // 跨行：在 content 全字节里找命中，再把 absolute byte offset 映射到 line+col
            let mut line_starts: Vec<usize> = Vec::with_capacity(lines.len() + 1);
            line_starts.push(0);
            for (i, b) in content.iter().enumerate() {
                if *b == b'\n' {
                    line_starts.push(i + 1);
                }
            }
            for m in re.find_iter(content) {
                let off = m.start();
                // line index via binary search
                let line_idx = match line_starts.binary_search(&off) {
                    Ok(i) => i,
                    Err(i) => i - 1,
                };
                let line = line_idx + 1;
                let col = off - line_starts[line_idx] + 1;
                out.push(Hit { line, col });
                if opts.first_only {
                    break;
                }
            }
        }
    }
    out
}

/// 把多文件命中按 `path:line[:col]` 写入 writer，返回总命中数（即输出行数）。
///
/// 顺序：保持 `files` 的输入顺序；同文件内按 `Hit` 生成顺序（即升序 line,col）。
pub fn write_hits<W: Write>(
    files_with_hits: &[(std::path::PathBuf, Vec<Hit>)],
    use_col: bool,
    max_matches: Option<usize>,
    mut out: W,
) -> Result<usize, FindError> {
    let cap = max_matches.unwrap_or(usize::MAX);
    let mut written = 0usize;
    for (path, hits) in files_with_hits {
        for h in hits {
            if written >= cap {
                return Ok(written);
            }
            if use_col && h.col > 0 {
                writeln!(out, "{}:{}:{}", path.display(), h.line, h.col)?;
            } else {
                writeln!(out, "{}:{}", path.display(), h.line)?;
            }
            written += 1;
        }
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn match_line_finds_all_hits() {
        let re = Regex::new(r"^foo").unwrap();
        let opts = FindOpts {
            mode: FindMode::MatchLine(&re),
            max_matches: None,
            first_only: false,
        };
        let hits = scan(b"foo\nbar\nfoo\n", &opts);
        assert_eq!(hits, vec![Hit { line: 1, col: 0 }, Hit { line: 3, col: 0 }]);
    }

    #[test]
    fn match_line_first_only_stops_at_one() {
        let re = Regex::new(r"^foo").unwrap();
        let opts = FindOpts {
            mode: FindMode::MatchLine(&re),
            max_matches: None,
            first_only: true,
        };
        let hits = scan(b"foo\nfoo\nfoo\n", &opts);
        assert_eq!(hits, vec![Hit { line: 1, col: 0 }]);
    }

    #[test]
    fn pattern_returns_byte_col() {
        let re = ByteRegex::new(r"foo").unwrap();
        let opts = FindOpts {
            mode: FindMode::Pattern(&re),
            max_matches: None,
            first_only: false,
        };
        let hits = scan(b"abc foo def\nxxfooyy\n", &opts);
        // line1 "abc foo def" → offset 4 → col 5
        // line2 "xxfooyy"     → offset 12 - line_start(12) → col 3
        assert_eq!(hits, vec![Hit { line: 1, col: 5 }, Hit { line: 2, col: 3 }]);
    }

    #[test]
    fn pattern_at_line_start_col_is_one() {
        let re = ByteRegex::new(r"foo").unwrap();
        let opts = FindOpts {
            mode: FindMode::Pattern(&re),
            max_matches: None,
            first_only: false,
        };
        let hits = scan(b"foo\n", &opts);
        assert_eq!(hits, vec![Hit { line: 1, col: 1 }]);
    }

    #[test]
    fn no_hits_returns_empty() {
        let re = Regex::new(r"xxx").unwrap();
        let opts = FindOpts {
            mode: FindMode::MatchLine(&re),
            max_matches: None,
            first_only: false,
        };
        assert!(scan(b"a\nb\n", &opts).is_empty());
    }

    #[test]
    fn write_hits_uses_path_line_for_match_line() {
        let mut buf = Vec::new();
        let entries = vec![(
            PathBuf::from("a.rs"),
            vec![Hit { line: 3, col: 0 }, Hit { line: 7, col: 0 }],
        )];
        let n = write_hits(&entries, false, None, &mut buf).unwrap();
        assert_eq!(n, 2);
        assert_eq!(String::from_utf8(buf).unwrap(), "a.rs:3\na.rs:7\n");
    }

    #[test]
    fn write_hits_uses_path_line_col_for_pattern() {
        let mut buf = Vec::new();
        let entries = vec![(PathBuf::from("a.rs"), vec![Hit { line: 3, col: 5 }])];
        let n = write_hits(&entries, true, None, &mut buf).unwrap();
        assert_eq!(n, 1);
        assert_eq!(String::from_utf8(buf).unwrap(), "a.rs:3:5\n");
    }

    #[test]
    fn write_hits_respects_max_matches() {
        let mut buf = Vec::new();
        let entries = vec![(
            PathBuf::from("a"),
            vec![
                Hit { line: 1, col: 0 },
                Hit { line: 2, col: 0 },
                Hit { line: 3, col: 0 },
            ],
        )];
        let n = write_hits(&entries, false, Some(2), &mut buf).unwrap();
        assert_eq!(n, 2);
        assert_eq!(String::from_utf8(buf).unwrap(), "a:1\na:2\n");
    }

    #[test]
    fn write_hits_iterates_multiple_files_in_order() {
        let mut buf = Vec::new();
        let entries = vec![
            (PathBuf::from("a"), vec![Hit { line: 1, col: 0 }]),
            (PathBuf::from("b"), vec![Hit { line: 2, col: 0 }]),
        ];
        let n = write_hits(&entries, false, None, &mut buf).unwrap();
        assert_eq!(n, 2);
        assert_eq!(String::from_utf8(buf).unwrap(), "a:1\nb:2\n");
    }
}
