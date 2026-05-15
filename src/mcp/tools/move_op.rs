//! `xnip_move` 工具：搬移连续行块。
//!
//! 与 cli `xnip move` 行为一致（PLAN §6.7.5），不暴露 dry-run/revert。

use std::path::PathBuf;

use regex::Regex;
use rmcp::{ErrorData as McpError, model::CallToolResult};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::core::atomic;
use crate::core::content;
use crate::core::location::{Locator, resolve};
use crate::core::ops::insert::Position;
use crate::core::ops::move_op::move_lines;

use super::common::{any_to_invalid_params, err_to_internal, locate_to_mcp_err};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct Args {
    /// 目标文件路径。
    pub file: PathBuf,

    /// 源行号区间，如 `"30-45"` 或单行 `"30"`（与 `from_match_line` 互斥）。
    #[serde(default)]
    pub from_lines: Option<String>,

    /// 源整行匹配正则。
    #[serde(default)]
    pub from_match_line: Option<String>,

    /// `from_match_line` 命中序号（1-based），默认 1。
    #[serde(default)]
    pub from_occurrence: Option<usize>,

    /// 目标行号（必填）。
    pub to: usize,

    /// 目标位置：`"before"` 或 `"after"`，默认 `"after"`。
    #[serde(default)]
    pub position: Option<String>,

    /// 写文件前先备份为 `<file>.bak`。
    #[serde(default)]
    pub backup: Option<bool>,
}

pub fn run(a: Args) -> Result<CallToolResult, McpError> {
    let backup = a.backup.unwrap_or(false);

    let bytes = content::load_path(&a.file).map_err(err_to_internal)?;

    // 解析源 Locator
    let src_loc = match (a.from_lines.as_deref(), a.from_match_line.as_deref()) {
        (Some(_), Some(_)) => {
            return Err(McpError::invalid_params(
                "`from_lines` and `from_match_line` are mutually exclusive",
                None,
            ));
        }
        (Some(s), None) => {
            let (start, end) =
                crate::cli::common::parse_line_range(s).map_err(any_to_invalid_params)?;
            Locator::Lines { start, end }
        }
        (None, Some(re)) => {
            let r = Regex::new(re).map_err(|e| {
                McpError::invalid_params(format!("invalid from_match_line regex: {e}"), None)
            })?;
            Locator::MatchLine {
                regex: r,
                occurrence: a.from_occurrence.unwrap_or(1).max(1),
            }
        }
        (None, None) => {
            return Err(McpError::invalid_params(
                "missing source: one of `from_lines` / `from_match_line` is required",
                None,
            ));
        }
    };

    let r = resolve(&src_loc, &bytes).map_err(locate_to_mcp_err)?;

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

    let new_bytes = move_lines(&bytes, r.start_line, r.end_line, a.to, position)
        .map_err(|e| McpError::invalid_request(format!("move_lines error: {e}"), None))?;

    atomic::atomic_write(&a.file, &new_bytes, backup).map_err(err_to_internal)?;

    let span = r.end_line + 1 - r.start_line;
    Ok(super::common::ok_text(format!(
        "xnip_move: wrote {} ({} line(s) relocated)",
        a.file.display(),
        span
    )))
}
