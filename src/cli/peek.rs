//! `xnip peek <file> [--lines a-b | --match-line RE [--context N] | --all] [--max-lines N]`
//!
//! PLAN.md §6.7.1。

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::Args as ClapArgs;
use regex::Regex;

use crate::core::content;
use crate::core::ops::peek::{PeekError, PeekOpts, PeekRange, run as run_peek};
use crate::output::exit;

/// peek 默认上限（PLAN §6.7.1：`--all` 默认 max-lines 1000）。
const DEFAULT_MAX_LINES_ALL: usize = 1000;

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// 目标文件路径。
    pub file: PathBuf,

    /// 行号区间，如 `30` 或 `30-45`。
    #[arg(long, value_name = "RANGE")]
    pub lines: Option<String>,

    /// 匹配行的正则。
    #[arg(long, value_name = "REGEX")]
    pub match_line: Option<String>,

    /// `--match-line` 上下文行数（前后各 N 行）。
    #[arg(long, default_value_t = 0, value_name = "N")]
    pub context: usize,

    /// 输出整个文件。
    #[arg(long, default_value_t = false)]
    pub all: bool,

    /// 最大输出行数；超出 stderr 提示。
    #[arg(long, value_name = "N")]
    pub max_lines: Option<usize>,
}

#[allow(clippy::needless_pass_by_value)]
pub fn run(a: Args) -> Result<u8> {
    // 互斥校验：lines / match-line / all 至少且最多 1
    let mut count = 0;
    if a.lines.is_some() {
        count += 1;
    }
    if a.match_line.is_some() {
        count += 1;
    }
    if a.all {
        count += 1;
    }
    if count == 0 {
        bail!("missing range: one of --lines / --match-line / --all is required");
    }
    if count > 1 {
        bail!("conflicting range: --lines / --match-line / --all are mutually exclusive");
    }

    // --context 仅当配 --match-line 才有意义
    if a.context > 0 && a.match_line.is_none() {
        bail!("--context can only be used with --match-line");
    }

    let range = if let Some(s) = a.lines {
        let (start, end) = super::common::parse_line_range(&s)?;
        PeekRange::Lines { start, end }
    } else if let Some(re) = a.match_line {
        let regex = Regex::new(&re).with_context(|| format!("invalid --match-line regex: {re}"))?;
        PeekRange::MatchLine {
            regex,
            context: a.context,
        }
    } else {
        PeekRange::All
    };

    // `--all` 默认 1000 上限；其它模式不设默认上限
    let max_lines = match (&range, a.max_lines) {
        (_, Some(n)) => Some(n),
        (PeekRange::All, None) => Some(DEFAULT_MAX_LINES_ALL),
        _ => None,
    };

    let bytes = content::load_path(&a.file).map_err(|e| anyhow::anyhow!("{e}"))?;
    let opts = PeekOpts { range, max_lines };

    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    match run_peek(&bytes, &opts, &mut out) {
        Ok(res) => {
            if res.truncated {
                eprintln!(
                    "xnip: output truncated to {} lines (use --max-lines to override)",
                    res.lines_written
                );
            }
            Ok(exit::SUCCESS)
        }
        Err(PeekError::Locate(e)) => {
            eprintln!("xnip: location not found: {e}");
            Ok(exit::USAGE)
        }
        Err(PeekError::Io(e)) => {
            eprintln!("xnip: io error: {e}");
            Ok(exit::IO)
        }
    }
}
