//! MCP adapter for bounded numbered reads.

use rmcp::{ErrorData as McpError, model::CallToolResult};
use schemars::JsonSchema;
use serde::Deserialize;
use std::path::PathBuf;

use super::common::{any_to_invalid_params, err_to_internal, io_to_mcp_err, locate_to_mcp_err};
use crate::core::{content, read::PeekQuery};

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Args {
    /// File to read.
    pub file: PathBuf,
    /// 1-based line or inclusive range, e.g. "30-45".
    #[serde(default)]
    pub lines: Option<String>,
    /// Line regex.
    #[serde(default)]
    pub match_line: Option<String>,
    /// Context on each side; requires match_line.
    #[serde(default)]
    pub context: Option<usize>,
    /// Read from the beginning.
    #[serde(default)]
    pub all: Option<bool>,
    /// Maximum lines; default 200, 0 unlimited.
    #[serde(default)]
    pub max_lines: Option<usize>,
    /// Maximum output bytes; default 12000, 0 unlimited.
    #[serde(default)]
    pub max_bytes: Option<usize>,
}

pub fn run(a: Args) -> Result<CallToolResult, McpError> {
    let query = PeekQuery::new(
        a.lines.as_deref(),
        a.match_line.as_deref(),
        a.all.unwrap_or(false),
        a.context.unwrap_or(0),
        a.max_lines,
        a.max_bytes,
    )
    .map_err(any_to_invalid_params)?;
    let bytes = content::load_path(&a.file).map_err(err_to_internal)?;
    let result = query.render(&bytes).map_err(|error| match error {
        crate::core::ops::peek::PeekError::Locate(error) => locate_to_mcp_err(error),
        crate::core::ops::peek::PeekError::Io(error) => io_to_mcp_err(error),
    })?;
    Ok(super::common::ok_text(result.into_text()))
}
