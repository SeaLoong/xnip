//! `xnip find <files...> [--pattern RE | --match-line RE] [--max-matches N] [--first-only]`
//!
//! PLAN.md §6.7.2。

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::Args as ClapArgs;
use regex::Regex;
use regex::bytes::Regex as ByteRegex;

use crate::core::content;
use crate::core::ops::find::{FindMode, FindOpts, scan, write_hits};
use crate::output::exit;

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// 要搜索的文件列表。
    #[arg(required = true, num_args = 1..)]
    pub files: Vec<PathBuf>,

    /// 字节级跨行命中正则。输出 `path:line:col`。
    #[arg(long, value_name = "REGEX", conflicts_with = "match_line")]
    pub pattern: Option<String>,

    /// 整行命中正则。输出 `path:line`。
    #[arg(long, value_name = "REGEX")]
    pub match_line: Option<String>,

    /// 总命中数上限。
    #[arg(long, value_name = "N")]
    pub max_matches: Option<usize>,

    /// 每个文件首次命中即停。
    #[arg(long, default_value_t = false)]
    pub first_only: bool,
}

#[allow(clippy::needless_pass_by_value)]
pub fn run(a: Args) -> Result<u8> {
    if a.pattern.is_none() && a.match_line.is_none() {
        bail!("missing locator: one of --pattern / --match-line is required");
    }

    // 编译一次正则
    let line_re = a
        .match_line
        .as_deref()
        .map(Regex::new)
        .transpose()
        .with_context(|| "invalid --match-line regex")?;
    let pat_re = a
        .pattern
        .as_deref()
        .map(ByteRegex::new)
        .transpose()
        .with_context(|| "invalid --pattern regex")?;

    let mode = if let Some(re) = &line_re {
        FindMode::MatchLine(re)
    } else {
        FindMode::Pattern(pat_re.as_ref().expect("locator presence checked"))
    };

    let opts = FindOpts {
        mode,
        max_matches: a.max_matches,
        first_only: a.first_only,
    };

    // 顺序扫描；M4 再加并发
    let mut files_with_hits = Vec::with_capacity(a.files.len());
    let mut total_hits = 0usize;
    for path in a.files {
        let bytes = match content::load_path(&path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("xnip find: {e}");
                continue;
            }
        };
        let hits = scan(&bytes, &opts);
        total_hits += hits.len();
        files_with_hits.push((path, hits));
    }

    let stdout = std::io::stdout();
    let out = stdout.lock();
    let use_col = a.pattern.is_some();
    let _written = write_hits(&files_with_hits, use_col, a.max_matches, out)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    if total_hits == 0 {
        // 与 grep 语义一致：未命中不算用户错误，但退码 ≠ 0 方便脚本判定
        // PLAN 未明确，这里取 USAGE（与"找不到定位"语义一致）
        return Ok(exit::USAGE);
    }
    Ok(exit::SUCCESS)
}
