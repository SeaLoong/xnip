//! `xnip indent <file> [--lines a-b | --all] (--add N | --remove N | --tabs-to-spaces N | --spaces-to-tabs N) [--dry-run] [--backup]`
//!
//! PLAN.md §6.7.6。

use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::Args as ClapArgs;

use crate::core::atomic;
use crate::core::content;
use crate::core::location::split_lines;
use crate::core::ops::indent::{IndentOp, apply_indent};
use crate::output::exit;

#[derive(Debug, ClapArgs)]
#[allow(clippy::struct_excessive_bools)]
pub struct Args {
    /// 目标文件路径。
    pub file: PathBuf,

    /// 行号区间，如 `30` / `30-45`。与 `--all` 互斥。
    #[arg(long, value_name = "RANGE", conflicts_with = "all")]
    pub lines: Option<String>,

    /// 应用到全文。
    #[arg(long, default_value_t = false)]
    pub all: bool,

    /// 每行行首加 N 个空格。
    #[arg(long, value_name = "N",
          conflicts_with_all = ["remove", "tabs_to_spaces", "spaces_to_tabs"])]
    pub add: Option<usize>,

    /// 每行行首删 N 个空格（不足则尽量删）。
    #[arg(long, value_name = "N",
          conflicts_with_all = ["tabs_to_spaces", "spaces_to_tabs"])]
    pub remove: Option<usize>,

    /// 行首每个 \t 展开为 N 个空格。
    #[arg(long, value_name = "N", conflicts_with = "spaces_to_tabs")]
    pub tabs_to_spaces: Option<usize>,

    /// 行首每 N 个连续空格折叠为 \t（剩余不足 N 的空格保留）。
    #[arg(long, value_name = "N")]
    pub spaces_to_tabs: Option<usize>,

    /// 仅打印到 stdout，不写文件。
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,

    /// 写文件前先备份为 `<file>.bak`。
    #[arg(long, default_value_t = false)]
    pub backup: bool,

    /// 反向执行：`Add(N)` ↔ `Remove(N)`；`TabsToSpaces(N)` ↔ `SpacesToTabs(N)`。
    /// 注意：`Remove` / `SpacesToTabs` forward 不严格可逆（可能丢信息），
    /// revert 后字节可能与原始不同。
    #[arg(long, default_value_t = false)]
    pub revert: bool,
}

#[allow(clippy::needless_pass_by_value)]
pub fn run(a: Args) -> Result<u8> {
    let bytes = content::load_path(&a.file).map_err(|e| anyhow::anyhow!("{e}"))?;

    // 1) 范围
    let total_lines = split_lines(&bytes).len();
    let (start, end) = match (&a.lines, a.all) {
        (Some(s), false) => super::common::parse_line_range(s)?,
        (None, true) => {
            if total_lines == 0 {
                // 空文件 + --all：直接退出无操作
                return Ok(exit::SUCCESS);
            }
            (1, total_lines)
        }
        (None, false) => bail!("missing range: one of --lines / --all is required"),
        (Some(_), true) => unreachable!("clap conflicts_with prevents this"),
    };

    // 2) 算子（必须且仅有一个）
    let mut count = 0;
    if a.add.is_some() {
        count += 1;
    }
    if a.remove.is_some() {
        count += 1;
    }
    if a.tabs_to_spaces.is_some() {
        count += 1;
    }
    if a.spaces_to_tabs.is_some() {
        count += 1;
    }
    if count == 0 {
        bail!(
            "missing op: one of --add / --remove / --tabs-to-spaces / --spaces-to-tabs is required"
        );
    }
    if count > 1 {
        bail!("only one indent op allowed at a time");
    }

    let op = if let Some(n) = a.add {
        IndentOp::Add(n)
    } else if let Some(n) = a.remove {
        IndentOp::Remove(n)
    } else if let Some(n) = a.tabs_to_spaces {
        IndentOp::TabsToSpaces(n)
    } else {
        IndentOp::SpacesToTabs(
            a.spaces_to_tabs
                .expect("count=1 ensures one of branches matches"),
        )
    };

    let effective_op = if a.revert {
        match op {
            IndentOp::Add(n) => IndentOp::Remove(n),
            IndentOp::Remove(n) => IndentOp::Add(n),
            IndentOp::TabsToSpaces(n) => IndentOp::SpacesToTabs(n),
            IndentOp::SpacesToTabs(n) => IndentOp::TabsToSpaces(n),
        }
    } else {
        op
    };

    let new_bytes =
        apply_indent(&bytes, start, end, effective_op).map_err(|e| anyhow::anyhow!("{e}"))?;

    if a.dry_run {
        use std::io::Write;
        std::io::stdout()
            .lock()
            .write_all(&new_bytes)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        return Ok(exit::SUCCESS);
    }

    atomic::atomic_write(&a.file, &new_bytes, a.backup).map_err(|e| anyhow::anyhow!("{e}"))?;
    let op_desc = match effective_op {
        IndentOp::Add(n) => format!("+{n} space(s)"),
        IndentOp::Remove(n) => format!("-{n} space(s)"),
        IndentOp::TabsToSpaces(n) => format!("tabs→{n} spaces"),
        IndentOp::SpacesToTabs(n) => format!("{n} spaces→tab"),
    };
    crate::note!(
        "xnip indent: wrote {} (lines {}-{}, {})",
        a.file.display(),
        start,
        end,
        op_desc
    );
    Ok(exit::SUCCESS)
}
