//! `xnip_apply` 工具：批量应用清单。
//!
//! 与 CLI `xnip apply` 共享计划，支持 dry_run/check；不读协议 stdin。
//! 输入支持两种来源：
//! - `path`：清单文件路径（支持 native/json/yaml/patch，按扩展名自动识别）
//! - `manifest_text`：行内清单文本（适合 LLM 直接生成短清单）
//!
//! 注意：MCP 上下文不读进程 stdin，因此清单中的 `@-` 内容来源**不被支持**——
//! 若清单 op 含 `@-`，会在解析后报错。LLM 应改用 `text` 字面或 `text_file`。

use std::path::PathBuf;

use rmcp::{ErrorData as McpError, model::CallToolResult};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::apply::commit::{ExecError, ExecOpts, execute_report};
use crate::apply::detect::{Format, parse_auto, parse_format_arg, parse_with};
use crate::apply::{FileOp, Op, OpContent, Target};

use super::common::{any_to_invalid_params, err_to_internal};

/// Direct exact replacements avoid encoding a manifest inside a JSON string.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExactEdit {
    pub file: PathBuf,
    pub old: String,
    pub text: String,
    #[serde(default)]
    pub all: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Args {
    /// Ordered exact replacements. Choose edits, path, or manifest_text.
    #[serde(default)]
    pub edits: Option<Vec<ExactEdit>>,
    /// Preview payload byte limit; default 12000, 0 for full output.
    #[serde(default)]
    pub max_bytes: Option<usize>,
    /// 清单文件路径（与 `manifest_text` 互斥）。
    #[serde(default)]
    pub path: Option<PathBuf>,

    /// 行内清单文本（与 `path` 互斥；适合短清单）。
    #[serde(default)]
    pub manifest_text: Option<String>,

    /// 显式格式：`"native"` / `"json"` / `"yaml"`（默认按扩展名 / 内容自动识别）。
    #[serde(default)]
    pub format: Option<String>,
    /// Explicit project directory for patch paths. Implies patch format; avoids relying on server cwd.
    #[serde(default)]
    pub patch_root: Option<PathBuf>,

    /// 写文件前先备份为 `<file>.bak`。
    #[serde(default)]
    pub backup: Option<bool>,

    /// 阶段一并行处理的文件数上限（0/1 = 单线程）。
    #[serde(default)]
    pub parallel: Option<usize>,
    #[serde(default)]
    pub sequential: bool,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub check: bool,
}

pub fn run(a: Args) -> Result<CallToolResult, McpError> {
    let backup = a.backup.unwrap_or(false);

    // 互斥与必填
    let (ops, manifest_dir) = if let Some(edits) = &a.edits {
        if a.path.is_some()
            || a.manifest_text.is_some()
            || a.format.is_some()
            || a.patch_root.is_some()
        {
            return Err(McpError::invalid_params(
                "edits conflicts with manifest inputs",
                None,
            ));
        }
        if edits.is_empty() {
            return Err(McpError::invalid_params(
                "edits must contain at least one replacement",
                None,
            ));
        }
        let ops = edits
            .iter()
            .map(|edit| Op::File {
                target: Target::File(edit.file.clone()),
                operation: FileOp::Exact {
                    old: OpContent::Text(edit.old.as_bytes().to_vec()),
                    text: OpContent::Text(edit.text.as_bytes().to_vec()),
                    all: edit.all,
                },
            })
            .collect();
        (ops, None)
    } else {
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
        let ops = if let Some(root) = &a.patch_root {
            if let Some(format) = &a.format
                && parse_format_arg(format).map_err(any_to_invalid_params)? != Format::Patch
            {
                return Err(McpError::invalid_params(
                    "patch_root requires patch format",
                    None,
                ));
            }
            crate::apply::patch::parse_in(&src, root).map_err(any_to_invalid_params)?
        } else if let Some(fmt_str) = a.format.as_deref() {
            let fmt = parse_format_arg(fmt_str).map_err(any_to_invalid_params)?;
            parse_with(&src, fmt).map_err(|e| {
                McpError::invalid_params(format!("parse with format {fmt:?} failed: {e:#}"), None)
            })?
        } else if a.path.is_some() {
            parse_auto(&src, a.path.as_deref()).map_err(any_to_invalid_params)?
        } else {
            // manifest_text 默认尝试 native，失败回落 auto-detect by content
            parse_with(&src, Format::Native)
                .or_else(|_| parse_auto(&src, None))
                .map_err(|e| {
                    McpError::invalid_params(format!("manifest_text parse failed: {e:#}"), None)
                })?
        };

        (ops, manifest_dir)
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
        check: a.check,
        dry_run: a.dry_run,
        sequential: a.sequential,
        backup,
        manifest_dir,
        stdin_bytes: None,
        parallel: match a.parallel {
            Some(n) if n > 1 => Some(n),
            _ => None,
        },
    };

    match execute_report(ops, &opts) {
        Ok(report) => {
            let summary = crate::output::human::ChangeSummary::batch(
                "apply",
                crate::output::human::Mode::new(a.dry_run, a.check),
                &report,
            );
            Ok(super::common::ok_change(
                &summary,
                if a.dry_run { &report.diff } else { "" },
                a.max_bytes,
            ))
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
        Op::File { operation, .. } => match operation {
            crate::apply::FileOp::Create(content)
            | crate::apply::FileOp::Exact { text: content, .. } => {
                matches!(content, OpContent::Stdin)
            }
            _ => false,
        },
        Op::Move { .. } | Op::Indent { .. } => false,
    })
}
