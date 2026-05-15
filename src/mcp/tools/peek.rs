//! `xnip_peek` 工具：打印带行号的指定区间。
//!
//! 与 cli `xnip peek` 行为一致（PLAN §6.7.1），输入 schema 字段名也保持一致。
//! 复用 `core::ops::peek::run`——它已是结构化的高级 API。

use std::path::PathBuf;

use regex::Regex;
use rmcp::{ErrorData as McpError, model::CallToolResult};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::core::content;
use crate::core::ops::peek::{PeekOpts, PeekRange, run as run_peek};

use super::common::{any_to_invalid_params, err_to_internal, io_to_mcp_err, locate_to_mcp_err};

/// `xnip_peek` 输入。互斥规则：`lines` / `match_line` / `all=true` 必须有且仅有一个。
#[derive(Debug, Deserialize, JsonSchema)]
pub struct Args {
    /// 目标文件路径（相对当前工作目录）。
    pub file: PathBuf,

    /// 行号区间字符串：`"30"` 或 `"30-45"`（1-based 闭区间）。
    #[serde(default)]
    pub lines: Option<String>,

    /// 整行匹配正则。例：`"^const PORT"`。
    #[serde(default)]
    pub match_line: Option<String>,

    /// `match_line` 上下文行数（前后各 N 行），仅与 `match_line` 配合。
    #[serde(default)]
    pub context: Option<usize>,

    /// 输出整个文件。
    #[serde(default)]
    pub all: Option<bool>,

    /// 最大输出行数；超出则截断（`all` 默认 1000，其它默认无上限）。
    #[serde(default)]
    pub max_lines: Option<usize>,
}

pub fn run(a: Args) -> Result<CallToolResult, McpError> {
    // 互斥校验
    let mut count = 0;
    if a.lines.is_some() {
        count += 1;
    }
    if a.match_line.is_some() {
        count += 1;
    }
    if a.all.unwrap_or(false) {
        count += 1;
    }
    if count == 0 {
        return Err(McpError::invalid_params(
            "missing range: one of `lines` / `match_line` / `all=true` is required",
            None,
        ));
    }
    if count > 1 {
        return Err(McpError::invalid_params(
            "conflicting range: `lines` / `match_line` / `all` are mutually exclusive",
            None,
        ));
    }

    let context = a.context.unwrap_or(0);
    if context > 0 && a.match_line.is_none() {
        return Err(McpError::invalid_params(
            "`context` can only be used with `match_line`",
            None,
        ));
    }

    let range = if let Some(s) = a.lines {
        let (start, end) =
            crate::cli::common::parse_line_range(&s).map_err(any_to_invalid_params)?;
        PeekRange::Lines { start, end }
    } else if let Some(re) = a.match_line {
        let regex = Regex::new(&re).map_err(|e| {
            McpError::invalid_params(format!("invalid match_line regex: {e}"), None)
        })?;
        PeekRange::MatchLine { regex, context }
    } else {
        PeekRange::All
    };

    // `--all` 默认 1000 上限（与 cli 保持一致）
    let max_lines = match (&range, a.max_lines) {
        (_, Some(n)) => Some(n),
        (PeekRange::All, None) => Some(1000),
        _ => None,
    };

    let bytes = content::load_path(&a.file).map_err(err_to_internal)?;
    let opts = PeekOpts { range, max_lines };

    let mut buf = Vec::new();
    let res = run_peek(&bytes, &opts, &mut buf).map_err(|e| match e {
        crate::core::ops::peek::PeekError::Locate(le) => locate_to_mcp_err(le),
        crate::core::ops::peek::PeekError::Io(io) => io_to_mcp_err(io),
    })?;

    let mut text = String::from_utf8_lossy(&buf).into_owned();
    if res.truncated {
        use std::fmt::Write as _;
        let _ = write!(
            text,
            "\n[xnip_peek] output truncated to {} line(s)\n",
            res.lines_written
        );
    }
    Ok(super::common::ok_text(text))
}
