//! `xnip_apply` 工具：批量应用清单。
//!
//! 与 cli `xnip apply` 行为一致（PLAN §6.9），不暴露 dry-run/check/json/from-stdin。
//! 输入支持两种来源：
//! - `path`：清单文件路径（支持 native/json/yaml，按扩展名自动识别）
//! - `manifest_text`：行内清单文本（适合 LLM 直接生成短清单）
//!
//! 注意：MCP 上下文不读进程 stdin，因此清单中的 `@-` 内容来源**不被支持**——
//! 若清单 op 含 `@-`，会在解析后报错。LLM 应改用 `text` 字面或 `text_file`。

use std::path::PathBuf;

use rmcp::{ErrorData as McpError, model::CallToolResult};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::apply::commit::{ExecError, ExecOpts, execute};
use crate::apply::detect::{Format, parse_auto, parse_format_arg, parse_with};
use crate::apply::{Op, OpContent};

use super::common::{any_to_invalid_params, err_to_internal};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct Args {
    /// 清单文件路径（与 `manifest_text` 互斥）。
    #[serde(default)]
    pub path: Option<PathBuf>,

    /// 行内清单文本（与 `path` 互斥；适合短清单）。
    #[serde(default)]
    pub manifest_text: Option<String>,

    /// 显式格式：`"native"` / `"json"` / `"yaml"`（默认按扩展名 / 内容自动识别）。
    #[serde(default)]
    pub format: Option<String>,

    /// 写文件前先备份为 `<file>.bak`。
    #[serde(default)]
    pub backup: Option<bool>,

    /// 阶段一并行处理的文件数上限（0/1 = 单线程）。
    #[serde(default)]
    pub parallel: Option<usize>,
}

pub fn run(a: Args) -> Result<CallToolResult, McpError> {
    let backup = a.backup.unwrap_or(false);

    // 互斥与必填
    let (src, manifest_dir) = match (a.path.as_deref(), a.manifest_text.as_deref()) {
        (Some(_), Some(_)) => {
            return Err(McpError::invalid_params(
                "`path` and `manifest_text` are mutually exclusive",
                None,
            ));
        }
        (Some(p), None) => {
            let bytes = std::fs::read(p).map_err(|e| {
                McpError::invalid_params(
                    format!("failed to read manifest {}: {e}", p.display()),
                    None,
                )
            })?;
            let s = String::from_utf8(bytes).map_err(|e| {
                McpError::invalid_params(format!("manifest must be UTF-8: {e}"), None)
            })?;
            let dir = p.parent().map(std::path::Path::to_path_buf);
            (s, dir)
        }
        (None, Some(t)) => (t.to_string(), None),
        (None, None) => {
            return Err(McpError::invalid_params(
                "either `path` or `manifest_text` is required",
                None,
            ));
        }
    };

    // 解析
    let ops = if let Some(fmt_str) = a.format.as_deref() {
        let fmt = parse_format_arg(fmt_str).map_err(any_to_invalid_params)?;
        parse_with(&src, fmt).map_err(|e| {
            McpError::invalid_params(format!("parse with format {fmt:?} failed: {e}"), None)
        })?
    } else if a.path.is_some() {
        parse_auto(&src, a.path.as_deref()).map_err(any_to_invalid_params)?
    } else {
        // manifest_text 默认尝试 native，失败回落 auto-detect by content
        parse_with(&src, Format::Native)
            .or_else(|_| parse_auto(&src, None))
            .map_err(|e| {
                McpError::invalid_params(format!("manifest_text parse failed: {e}"), None)
            })?
    };

    // MCP 不允许 op 内 `@-`（无法读进程 stdin）
    if has_stdin_content(&ops) {
        return Err(McpError::invalid_params(
            "manifest contains op content `@-` which reads stdin; \
             MCP server cannot consume stdin (it is occupied by the protocol). \
             Use `text` literal or `text_file` instead.",
            None,
        ));
    }

    // 执行
    let opts = ExecOpts {
        check: false,
        dry_run: false,
        backup,
        manifest_dir,
        stdin_bytes: None,
        parallel: match a.parallel {
            Some(n) if n > 1 => Some(n),
            _ => None,
        },
    };

    match execute(ops, &opts) {
        Ok(files) => {
            let mut text = format!("xnip_apply: committed {} file(s)\n", files.len());
            for f in &files {
                text.push_str("  - ");
                text.push_str(&f.display().to_string());
                text.push('\n');
            }
            Ok(super::common::ok_text(text))
        }
        Err(e @ ExecError::Phase1 { .. }) => {
            Err(McpError::invalid_request(format!("phase1: {e}"), None))
        }
        Err(e @ ExecError::Phase2 { .. }) => Err(McpError::internal_error(
            format!("phase2 (partial commit): {e}"),
            None,
        )),
        Err(e @ ExecError::Io(_)) => Err(err_to_internal(e)),
    }
}

fn has_stdin_content(ops: &[Op]) -> bool {
    ops.iter().any(|op| match op {
        Op::Replace { content, .. } | Op::Insert { content, .. } => {
            matches!(content, OpContent::Stdin)
        }
        Op::Move { .. } | Op::Indent { .. } => false,
    })
}
