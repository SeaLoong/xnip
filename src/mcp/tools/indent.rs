//! `xnip_indent` 工具：调整行首缩进 / tabs↔spaces 转换。
//!
//! 与 cli `xnip indent` 行为一致（PLAN §6.7.6）。

use std::path::PathBuf;

use rmcp::{ErrorData as McpError, model::CallToolResult};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::core::atomic;
use crate::core::content;
use crate::core::location::split_lines;
use crate::core::ops::indent::{IndentOp, apply_indent};

use super::common::{any_to_invalid_params, err_to_internal};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct Args {
    /// 目标文件路径。
    pub file: PathBuf,

    /// 行号区间，如 `"30"` / `"30-45"`（与 `all` 互斥）。
    #[serde(default)]
    pub lines: Option<String>,

    /// 应用到全文。
    #[serde(default)]
    pub all: Option<bool>,

    /// 每行行首加 N 个空格。
    #[serde(default)]
    pub add: Option<usize>,

    /// 每行行首删 N 个空格（不足则尽量删）。
    #[serde(default)]
    pub remove: Option<usize>,

    /// 行首每个 `\t` 展开为 N 个空格。
    #[serde(default)]
    pub tabs_to_spaces: Option<usize>,

    /// 行首每 N 个连续空格折叠为 `\t`。
    #[serde(default)]
    pub spaces_to_tabs: Option<usize>,

    /// 写文件前先备份为 `<file>.bak`。
    #[serde(default)]
    pub backup: Option<bool>,
}

pub fn run(a: Args) -> Result<CallToolResult, McpError> {
    let backup = a.backup.unwrap_or(false);

    let bytes = content::load_path(&a.file).map_err(err_to_internal)?;

    // 范围
    let total_lines = split_lines(&bytes).len();
    let (start, end) = match (a.lines.as_deref(), a.all.unwrap_or(false)) {
        (Some(_), true) => {
            return Err(McpError::invalid_params(
                "`lines` and `all` are mutually exclusive",
                None,
            ));
        }
        (Some(s), false) => {
            crate::cli::common::parse_line_range(s).map_err(any_to_invalid_params)?
        }
        (None, true) => {
            if total_lines == 0 {
                return Ok(super::common::ok_text(
                    "xnip_indent: no-op (empty file with `all`)",
                ));
            }
            (1, total_lines)
        }
        (None, false) => {
            return Err(McpError::invalid_params(
                "missing range: one of `lines` / `all=true` is required",
                None,
            ));
        }
    };

    // 算子（必须且仅有一个）
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
        return Err(McpError::invalid_params(
            "missing op: one of `add` / `remove` / `tabs_to_spaces` / `spaces_to_tabs` is required",
            None,
        ));
    }
    if count > 1 {
        return Err(McpError::invalid_params(
            "only one indent op allowed at a time",
            None,
        ));
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
                .expect("count=1 ensures one branch matches"),
        )
    };

    let new_bytes = apply_indent(&bytes, start, end, op)
        .map_err(|e| McpError::invalid_request(format!("apply_indent error: {e}"), None))?;

    atomic::atomic_write(&a.file, &new_bytes, backup).map_err(err_to_internal)?;

    let op_desc = match op {
        IndentOp::Add(n) => format!("+{n} space(s)"),
        IndentOp::Remove(n) => format!("-{n} space(s)"),
        IndentOp::TabsToSpaces(n) => format!("tabs→{n} spaces"),
        IndentOp::SpacesToTabs(n) => format!("{n} spaces→tab"),
    };
    Ok(super::common::ok_text(format!(
        "xnip_indent: wrote {} (lines {start}-{end}, {op_desc})",
        a.file.display()
    )))
}
