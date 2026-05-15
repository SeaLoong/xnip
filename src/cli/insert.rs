//! `xnip insert <file> [Locator] [--position before|after] [--text...]`
//!
//! PLAN.md §6.7.4。Locator 必须解析为单点行（`Lines a-a` 即可，区间用 `replace`）。

use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::Args as ClapArgs;
use clap::ValueEnum;

use crate::core::atomic;
use crate::core::content;
use crate::core::location::{Locator, resolve};
use crate::core::ops::insert::{Position, insert_at};
use crate::output::exit;

use super::common::{ContentArgs, LocatorArgs};

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

    #[command(flatten)]
    pub locator: LocatorArgs,

    /// 插入位置（默认 after）。
    #[arg(long, value_enum, default_value_t = PositionArg::After)]
    pub position: PositionArg,

    #[command(flatten)]
    pub content: ContentArgs,

    /// 仅打印新内容到 stdout，不写文件。
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,

    /// 写文件前先备份为 `<file>.bak`（默认不写）。
    #[arg(long, default_value_t = false)]
    pub backup: bool,

    /// 反向执行：仅支持 `--lines A`（单行错点）。
    /// 会根据 `--position` 与 payload 行数计算要删除的区间，
    /// 前置校验该区间字节严格等于 payload（不匹配报 exit 3）。
    #[arg(long, default_value_t = false)]
    pub revert: bool,
}

#[allow(clippy::needless_pass_by_value, clippy::naive_bytecount)]
pub fn run(a: Args) -> Result<u8> {
    // 1) 读原文件
    let bytes = content::load_path(&a.file).map_err(|e| anyhow::anyhow!("{e}"))?;

    // 2) 解析 Locator → 必须是单行（区间走 replace）
    let loc = a.locator.into_locator()?;
    if matches!(loc, Locator::Pattern { .. }) {
        bail!("`insert` does not accept --pattern; use `replace` for pattern-based edits");
    }
    let r = resolve(&loc, &bytes).map_err(|e| anyhow::anyhow!("{e}"))?;
    if r.start_line != r.end_line {
        bail!(
            "`insert` requires a single-line anchor; got range {}-{}. Use `replace` for range edits.",
            r.start_line,
            r.end_line
        );
    }

    // 3) 加载内容
    let content_src = a
        .content
        .into_content(true)?
        .expect("require_some=true ensures Some");
    if content_src.as_replacement().is_some() {
        bail!("`insert` does not accept --repl; use --text / --text-stdin / --text-file");
    }
    let payload = content::load(&content_src).map_err(|e| anyhow::anyhow!("{e}"))?;

    // 4) 计算新内容
    let new_bytes = if a.revert {
        // revert 仅支持 --lines A 单行 locator
        if !matches!(loc, Locator::Lines { .. }) {
            bail!(
                "`--revert` for insert requires `--lines A` locator (anchor by exact line number)"
            );
        }
        // 计算 forward 写入的字节（normalize 后的 payload）与行数
        let normalized = if payload.is_empty() {
            Vec::new()
        } else if payload.last() == Some(&b'\n') {
            payload.clone()
        } else {
            let mut v = payload.clone();
            v.push(b'\n');
            v
        };
        let line_count = normalized.iter().filter(|&&b| b == b'\n').count();
        if line_count == 0 {
            bail!("`--revert` insert: payload has zero lines, nothing to remove");
        }
        let (del_start, del_end) = match Position::from(a.position) {
            Position::After => (r.start_line + 1, r.start_line + line_count),
            Position::Before => (r.start_line, r.start_line + line_count - 1),
        };
        // 前置校验：该区间严格等于 normalized payload
        let actual =
            crate::core::location::extract_line_range_with_newline(&bytes, del_start, del_end);
        if actual != normalized {
            eprintln!(
                "xnip insert: --revert pre-condition failed at lines {del_start}-{del_end} \
                 (expected current content == --text)"
            );
            return Ok(exit::CHECK);
        }
        crate::core::ops::replace::replace_range(&bytes, del_start, del_end, b"")
            .map_err(|e| anyhow::anyhow!("{e}"))?
    } else {
        insert_at(&bytes, r.start_line, a.position.into(), &payload)
            .map_err(|e| anyhow::anyhow!("{e}"))?
    };

    // 5) dry-run / 写盘
    if a.dry_run {
        use std::io::Write;
        std::io::stdout()
            .lock()
            .write_all(&new_bytes)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        return Ok(exit::SUCCESS);
    }

    atomic::atomic_write(&a.file, &new_bytes, a.backup).map_err(|e| anyhow::anyhow!("{e}"))?;
    crate::note!(
        "xnip insert: wrote {} ({} byte(s))",
        a.file.display(),
        new_bytes.len()
    );
    Ok(exit::SUCCESS)
}
