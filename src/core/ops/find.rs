//! Ordered regex search over borrowed bytes.
//!
//! Line searches return path:line; cross-line byte patterns also return a
//! 1-based byte column. Visitors can stop early without retaining hit lists.

use std::io::Write;

use regex::Regex;
use regex::bytes::Regex as ByteRegex;
use thiserror::Error;

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

/// Collect matches for callers that need the complete bounded hit list.
pub fn scan(content: &[u8], opts: &FindOpts<'_>) -> Vec<Hit> {
    let mut hits = Vec::new();
    visit(content, opts, |hit| {
        hits.push(hit);
        true
    });
    hits
}

/// Visit matches in order without retaining them. Return false to stop scanning.
pub fn visit(content: &[u8], opts: &FindOpts<'_>, mut on_hit: impl FnMut(Hit) -> bool) {
    let cap =
        if opts.first_only { 1 } else { usize::MAX }.min(opts.max_matches.unwrap_or(usize::MAX));
    if cap == 0 {
        return;
    }
    match opts.mode {
        FindMode::MatchLine(re) => {
            let mut matched = 0;
            for (idx, line) in content.split_inclusive(|b| *b == b'\n').enumerate() {
                let line = line.strip_suffix(b"\n").unwrap_or(line);
                if re.is_match(&String::from_utf8_lossy(line)) {
                    matched += 1;
                    if !on_hit(Hit {
                        line: idx + 1,
                        col: 0,
                    }) || matched == cap
                    {
                        break;
                    }
                }
            }
        }
        FindMode::Pattern(re) => {
            let mut cursor = 0;
            let mut line = 1;
            let mut line_start = 0;
            for found in re.find_iter(content).take(cap) {
                for (index, byte) in content[cursor..found.start()].iter().enumerate() {
                    if *byte == b'\n' {
                        line += 1;
                        line_start = cursor + index + 1;
                    }
                }
                cursor = found.start();
                if !on_hit(Hit {
                    line,
                    col: found.start() - line_start + 1,
                }) {
                    break;
                }
            }
        }
    }
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
