//! `xnip_replace` 工具：替换/删除一段区域。
//!
//! 与 cli `xnip replace` 行为一致（PLAN §6.7.3），但**不暴露 dry-run/check/revert**——
//! 这些是 cli 便利特性，对 LLM 直接构造正向编辑没价值。
//!
//! 保留 `was`/`was_file`（并发保护）与 `backup`（用户安全旁路）。

use std::path::PathBuf;

use rmcp::{ErrorData as McpError, model::CallToolResult};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::core::atomic;
use crate::core::content;
use crate::core::location::{Locator, resolve};
use crate::core::ops::replace::{replace_pattern, replace_range};

use super::common::{ContentInput, LocatorInput, err_to_internal, locate_to_mcp_err};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct Args {
    /// 目标文件路径。
    pub file: PathBuf,

    /// 5 维定位（5 选 1）。
    #[serde(flatten)]
    pub locator: LocatorInput,

    /// 内容来源（`text` / `text_file` / `repl`）。
    #[serde(flatten)]
    pub content: ContentInput,

    /// 校验：原区段必须等于此字符串字面（与 `was_file` 互斥）。
    #[serde(default)]
    pub was: Option<String>,

    /// 校验：原区段必须等于此文件内容。
    #[serde(default)]
    pub was_file: Option<PathBuf>,

    /// 写文件前先备份为 `<file>.bak`。
    #[serde(default)]
    pub backup: Option<bool>,
}

pub fn run(a: Args) -> Result<CallToolResult, McpError> {
    let backup = a.backup.unwrap_or(false);

    // 1) 读原文件
    let bytes = content::load_path(&a.file).map_err(err_to_internal)?;

    // 2) 解析 Locator
    let loc = a.locator.into_locator()?;

    // 3) 准备 content（必填）
    let content_src = a
        .content
        .into_content(true)?
        .expect("require_some=true ensures Some");

    // 4) 分两路：pattern vs range
    let new_bytes = match loc {
        Locator::Pattern { regex, count } => {
            let repl = content_src.as_replacement().ok_or_else(|| {
                McpError::invalid_params("`pattern` requires `repl`; got `text`/`text_file`", None)
            })?;
            let re = regex::bytes::Regex::new(regex.as_str()).map_err(|e| {
                McpError::invalid_params(format!("invalid pattern after revert: {e}"), None)
            })?;
            let (new_bytes, n_replaced) = replace_pattern(&bytes, &re, repl, count);
            if n_replaced == 0 {
                return Err(McpError::invalid_request(
                    "pattern did not match anything",
                    None,
                ));
            }
            new_bytes
        }
        loc => {
            // range 模式
            let r = resolve(&loc, &bytes).map_err(locate_to_mcp_err)?;

            // was / was_file 校验
            if let Some(expected) = expected_was(a.was.as_ref(), a.was_file.as_ref())? {
                let actual = crate::core::location::extract_line_range_with_newline(
                    &bytes,
                    r.start_line,
                    r.end_line,
                );
                if actual != expected {
                    return Err(McpError::invalid_request(
                        format!(
                            "`was` check failed at lines {}-{} (current content does not match)",
                            r.start_line, r.end_line
                        ),
                        None,
                    ));
                }
            }

            if content_src.as_replacement().is_some() {
                return Err(McpError::invalid_params(
                    "`repl` is only valid with `pattern`",
                    None,
                ));
            }
            let payload = content::load(&content_src).map_err(err_to_internal)?;

            replace_range(&bytes, r.start_line, r.end_line, &payload)
                .map_err(|e| McpError::invalid_request(format!("replace_range error: {e}"), None))?
        }
    };

    // 5) 原子写
    atomic::atomic_write(&a.file, &new_bytes, backup).map_err(err_to_internal)?;

    Ok(super::common::ok_text(format!(
        "xnip_replace: wrote {} ({} byte(s))",
        a.file.display(),
        new_bytes.len()
    )))
}

fn expected_was(
    was: Option<&String>,
    was_file: Option<&PathBuf>,
) -> Result<Option<Vec<u8>>, McpError> {
    match (was, was_file) {
        (Some(_), Some(_)) => Err(McpError::invalid_params(
            "`was` and `was_file` are mutually exclusive",
            None,
        )),
        (Some(s), None) => Ok(Some(s.clone().into_bytes())),
        (None, Some(p)) => std::fs::read(p)
            .map(Some)
            .map_err(|e| McpError::internal_error(format!("failed to read was_file: {e}"), None)),
        (None, None) => Ok(None),
    }
}
