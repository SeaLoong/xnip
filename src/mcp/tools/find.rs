//! MCP adapter for bounded multi-file search.

use rmcp::{ErrorData as McpError, model::CallToolResult};
use schemars::JsonSchema;
use serde::Deserialize;
use std::path::PathBuf;

use super::common::any_to_invalid_params;
use crate::core::read::FindQuery;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Args {
    /// Files to search, in output order.
    pub files: Vec<PathBuf>,
    /// Byte regex; emits path:line:column.
    #[serde(default)]
    pub pattern: Option<String>,
    /// Line regex; emits path:line.
    #[serde(default)]
    pub match_line: Option<String>,
    /// Maximum total hits; default 100, 0 unlimited.
    #[serde(default)]
    pub max_matches: Option<usize>,
    /// Maximum output bytes; default 12000, 0 unlimited.
    #[serde(default)]
    pub max_bytes: Option<usize>,
    /// Keep only the first hit per file.
    #[serde(default)]
    pub first_only: Option<bool>,
}

pub fn run(a: Args) -> Result<CallToolResult, McpError> {
    let result = FindQuery::new(
        a.pattern.as_deref(),
        a.match_line.as_deref(),
        a.first_only.unwrap_or(false),
        a.max_matches,
        a.max_bytes,
    )
    .and_then(|query| query.run(&a.files))
    .map_err(any_to_invalid_params)?;
    if result.items == 0 && result.load_error_count > 0 {
        return Err(McpError::invalid_params(result.into_text(), None));
    }
    if result.items == 0 {
        return Ok(super::common::ok_text("(no matches)"));
    }
    Ok(super::common::ok_text(result.into_text()))
}
