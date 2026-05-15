//! `xnip_insert` 工具：在单行锚点的前/后插入内容。
//!
//! 与 cli `xnip insert` 行为一致（PLAN §6.7.4），不暴露 dry-run/check/revert/backup_revert。

use std::path::PathBuf;

use rmcp::{ErrorData as McpError, model::CallToolResult};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::core::atomic;
use crate::core::content;
use crate::core::location::{Locator, resolve};
use crate::core::ops::insert::{Position, insert_at};

use super::common::{ContentInput, LocatorInput, err_to_internal, locate_to_mcp_err};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct Args {
    /// 目标文件路径。
    pub file: PathBuf,

    /// 单行锚点定位（`pattern` 不支持）。
    #[serde(flatten)]
    pub locator: LocatorInput,

    /// 插入位置：`"before"` 或 `"after"`，默认 `"after"`。
    #[serde(default)]
    pub position: Option<String>,

    /// 内容来源（`text` / `text_file`，不接受 `repl`）。
    #[serde(flatten)]
    pub content: ContentInput,

    /// 写文件前先备份为 `<file>.bak`。
    #[serde(default)]
    pub backup: Option<bool>,
}

pub fn run(a: Args) -> Result<CallToolResult, McpError> {
    let backup = a.backup.unwrap_or(false);

    // 1) 读原文件
    let bytes = content::load_path(&a.file).map_err(err_to_internal)?;

    // 2) 解析 Locator → 必须是单行
    let loc = a.locator.into_locator()?;
    if matches!(loc, Locator::Pattern { .. }) {
        return Err(McpError::invalid_params(
            "`insert` does not accept `pattern`; use `replace` for pattern-based edits",
            None,
        ));
    }
    let r = resolve(&loc, &bytes).map_err(locate_to_mcp_err)?;
    if r.start_line != r.end_line {
        return Err(McpError::invalid_params(
            format!(
                "`insert` requires a single-line anchor; got range {}-{}. Use `replace` for ranges.",
                r.start_line, r.end_line
            ),
            None,
        ));
    }

    // 3) 内容
    let content_src = a
        .content
        .into_content(true)?
        .expect("require_some=true ensures Some");
    if content_src.as_replacement().is_some() {
        return Err(McpError::invalid_params(
            "`insert` does not accept `repl`; use `text` or `text_file`",
            None,
        ));
    }
    let payload = content::load(&content_src).map_err(err_to_internal)?;

    // 4) 位置
    let position = match a
        .position
        .as_deref()
        .unwrap_or("after")
        .to_ascii_lowercase()
        .as_str()
    {
        "before" => Position::Before,
        "after" => Position::After,
        other => {
            return Err(McpError::invalid_params(
                format!("invalid `position`: {other:?}; expected 'before' or 'after'"),
                None,
            ));
        }
    };

    // 5) 计算并写入
    let new_bytes = insert_at(&bytes, r.start_line, position, &payload)
        .map_err(|e| McpError::invalid_request(format!("insert_at error: {e}"), None))?;
    atomic::atomic_write(&a.file, &new_bytes, backup).map_err(err_to_internal)?;

    Ok(super::common::ok_text(format!(
        "xnip_insert: wrote {} ({} byte(s))",
        a.file.display(),
        new_bytes.len()
    )))
}
