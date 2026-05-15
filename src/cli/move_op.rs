//! `xnip move <file> --from <range-locator> --to <line> [--position before|after] [--dry-run] [--backup]`
//!
//! PLAN.md §6.7.5。

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::Args as ClapArgs;
use clap::ValueEnum;
use regex::Regex;

use crate::core::atomic;
use crate::core::content;
use crate::core::location::{Locator, resolve};
use crate::core::ops::insert::Position;
use crate::core::ops::move_op::move_lines;
use crate::output::exit;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum PositionArg {
    Before,
    After,
}

impl From<PositionArg> for Position {
    fn from(p: PositionArg) -> Self {
        match p {
            PositionArg::Before => Position::Before,
            PositionArg::After => Position::After,
        }
    }
}

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// 目标文件路径。
    pub file: PathBuf,

    /// 源行号区间，如 `30-45`，或单行 `30`。
    #[arg(long, value_name = "RANGE")]
    pub from_lines: Option<String>,

    /// 源整行匹配正则。
    #[arg(long, value_name = "REGEX")]
    pub from_match_line: Option<String>,

    /// `--from-match-line` 命中序号（1-based），默认 1。
    #[arg(long, default_value_t = 1, value_name = "N")]
    pub from_occurrence: usize,

    /// 目标行号（必填）。
    #[arg(long, value_name = "N")]
    pub to: usize,

    /// 目标位置（默认 after）。
    #[arg(long, value_enum, default_value_t = PositionArg::After)]
    pub position: PositionArg,

    /// 仅打印到 stdout，不写文件。
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,

    /// 写文件前先备份为 `<file>.bak`。
    #[arg(long, default_value_t = false)]
    pub backup: bool,

    /// 反向执行：以相同参数调用时，会计算 forward 后块在新内容里的位置，再搬回原位。
    #[arg(long, default_value_t = false)]
    pub revert: bool,
}

#[allow(clippy::needless_pass_by_value)]
pub fn run(a: Args) -> Result<u8> {
    let bytes = content::load_path(&a.file).map_err(|e| anyhow::anyhow!("{e}"))?;

    // 解析源 Locator（仅支持 lines / match-line；其它写法走 apply）
    let src_loc = match (&a.from_lines, &a.from_match_line) {
        (Some(_), Some(_)) => bail!("--from-lines and --from-match-line are mutually exclusive"),
        (Some(s), None) => {
            let (start, end) = super::common::parse_line_range(s)?;
            Locator::Lines { start, end }
        }
        (None, Some(re)) => {
            let r =
                Regex::new(re).with_context(|| format!("invalid --from-match-line regex: {re}"))?;
            Locator::MatchLine {
                regex: r,
                occurrence: a.from_occurrence.max(1),
            }
        }
        (None, None) => {
            bail!("missing source: one of --from-lines / --from-match-line is required");
        }
    };

    // --revert 的安全性约束：源 locator 必须是 `--from-lines`（行号是绝对的）。
    // `--from-match-line` 在 forward 之后定位的是新位置，不再指向原源块；
    // 用其参数计算反向会得到错误结果。这里显式拦截。
    if a.revert && !matches!(src_loc, Locator::Lines { .. }) {
        bail!(
            "`--revert` for move requires `--from-lines S-E` locator; \
             `--from-match-line` cannot be inverted because the anchor \
             refers to a different block in the post-forward file"
        );
    }

    let r = resolve(&src_loc, &bytes).map_err(|e| anyhow::anyhow!("{e}"))?;

    let (eff_s, eff_e, eff_to, eff_pos) = if a.revert {
        crate::core::ops::move_op::reverse_params(r.start_line, r.end_line, a.to, a.position.into())
            .map_err(|e| anyhow::anyhow!("{e}"))?
    } else {
        (r.start_line, r.end_line, a.to, a.position.into())
    };

    let new_bytes =
        move_lines(&bytes, eff_s, eff_e, eff_to, eff_pos).map_err(|e| anyhow::anyhow!("{e}"))?;

    if a.dry_run {
        use std::io::Write;
        std::io::stdout()
            .lock()
            .write_all(&new_bytes)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        return Ok(exit::SUCCESS);
    }

    atomic::atomic_write(&a.file, &new_bytes, a.backup).map_err(|e| anyhow::anyhow!("{e}"))?;
    let span = eff_e + 1 - eff_s;
    crate::note!(
        "xnip move: wrote {} ({} line(s) relocated)",
        a.file.display(),
        span
    );
    Ok(exit::SUCCESS)
}
