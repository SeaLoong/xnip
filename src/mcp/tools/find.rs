//! `xnip_find` 工具：在多个文件里搜索定位。
//!
//! 与 cli `xnip find` 一致（PLAN §6.7.2）。复用 `core::ops::find::{scan, write_hits}`。

use std::path::PathBuf;

use regex::Regex;
use regex::bytes::Regex as ByteRegex;
use rmcp::{ErrorData as McpError, model::CallToolResult};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::core::content;
use crate::core::ops::find::{FindMode, FindOpts, scan, write_hits};

use super::common::{err_to_internal, ok_bytes_as_text};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct Args {
    /// 要搜索的文件路径列表（必填，至少 1 个）。
    pub files: Vec<PathBuf>,

    /// 字节级跨行命中正则（与 `match_line` 互斥）。命中输出 `path:line:col`。
    #[serde(default)]
    pub pattern: Option<String>,

    /// 整行命中正则。命中输出 `path:line`。
    #[serde(default)]
    pub match_line: Option<String>,

    /// 总命中数上限。
    #[serde(default)]
    pub max_matches: Option<usize>,

    /// 每个文件首次命中即停。
    #[serde(default)]
    pub first_only: Option<bool>,
}

pub fn run(a: Args) -> Result<CallToolResult, McpError> {
    if a.files.is_empty() {
        return Err(McpError::invalid_params(
            "`files` must contain at least one path",
            None,
        ));
    }
    if a.pattern.is_none() && a.match_line.is_none() {
        return Err(McpError::invalid_params(
            "missing locator: one of `pattern` / `match_line` is required",
            None,
        ));
    }
    if a.pattern.is_some() && a.match_line.is_some() {
        return Err(McpError::invalid_params(
            "`pattern` and `match_line` are mutually exclusive",
            None,
        ));
    }

    // 编译正则
    let line_re = a
        .match_line
        .as_deref()
        .map(Regex::new)
        .transpose()
        .map_err(|e| McpError::invalid_params(format!("invalid match_line regex: {e}"), None))?;
    let pat_re = a
        .pattern
        .as_deref()
        .map(ByteRegex::new)
        .transpose()
        .map_err(|e| McpError::invalid_params(format!("invalid pattern regex: {e}"), None))?;

    let mode = if let Some(re) = &line_re {
        FindMode::MatchLine(re)
    } else {
        FindMode::Pattern(pat_re.as_ref().expect("locator presence checked"))
    };

    let opts = FindOpts {
        mode,
        max_matches: a.max_matches,
        first_only: a.first_only.unwrap_or(false),
    };

    let mut files_with_hits = Vec::with_capacity(a.files.len());
    let mut total_hits = 0usize;
    let mut load_errors: Vec<String> = Vec::new();
    for path in a.files {
        match content::load_path(&path) {
            Ok(bytes) => {
                let hits = scan(&bytes, &opts);
                total_hits += hits.len();
                files_with_hits.push((path, hits));
            }
            Err(e) => {
                load_errors.push(format!("{}: {e}", path.display()));
            }
        }
    }

    let use_col = a.pattern.is_some();
    let mut buf = Vec::new();
    write_hits(&files_with_hits, use_col, a.max_matches, &mut buf).map_err(err_to_internal)?;

    if total_hits == 0 {
        if !load_errors.is_empty() {
            // 文件全打不开 → 当作错误
            return Err(McpError::invalid_params(
                format!("no matches; load errors: {}", load_errors.join("; ")),
                None,
            ));
        }
        return Ok(super::common::ok_text("(no matches)"));
    }

    // 命中文本 + 末尾追加 load errors（如有）
    let mut text = String::from_utf8_lossy(&buf).into_owned();
    if !load_errors.is_empty() {
        text.push_str("\n[xnip_find] partial load errors:\n");
        for e in &load_errors {
            text.push_str("  - ");
            text.push_str(e);
            text.push('\n');
        }
    }
    let _ = ok_bytes_as_text; // suppress unused warning if any
    Ok(super::common::ok_text(text))
}
