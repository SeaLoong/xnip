use crate::core::edit::{EditError, Operation, Request, execute, payload};
use rmcp::{ErrorData as McpError, model::CallToolResult};
use schemars::JsonSchema;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(clippy::struct_excessive_bools)]
pub struct Args {
    /// Preview payload byte limit; default 12000, 0 for full output.
    #[serde(default)]
    pub max_bytes: Option<usize>,
    /// Target path; create requires an absent path, other operations an existing file.
    pub file: PathBuf,
    /// replace (default), create (must not exist), delete (regular files only), or rename (to required).
    #[serde(default)]
    pub operation: Operation,
    /// Absent destination for rename. Same filesystem required.
    pub to: Option<PathBuf>,
    /// Exact old block; for delete, optional expected whole-file content.
    pub old: Option<String>,
    /// File containing exact old bytes (mutually exclusive with old).
    pub old_file: Option<PathBuf>,
    /// Replacement/new file bytes. Empty string is allowed.
    pub text: Option<String>,
    /// File containing new bytes (mutually exclusive with text).
    pub text_file: Option<PathBuf>,
    /// Explicitly replace every non-overlapping match instead of requiring uniqueness.
    #[serde(default)]
    pub all: bool,
    /// Return a diff without writing, including for create/delete.
    #[serde(default)]
    pub dry_run: bool,
    /// Validate without writing or returning a diff; conflicts with dry_run.
    #[serde(default)]
    pub check: bool,
    /// Save original bytes to <file>.bak before replacement or deletion.
    #[serde(default)]
    pub backup: bool,
}

fn error(error: EditError) -> McpError {
    match error {
        EditError::Usage(message) => McpError::invalid_params(message, None),
        EditError::Check(message) => McpError::invalid_request(message, None),
        other => McpError::internal_error(other.to_string(), None),
    }
}

pub fn run(args: Args) -> Result<CallToolResult, McpError> {
    let request = Request {
        operation: args.operation,
        to: args.to,
        old: payload(args.old, args.old_file).map_err(error)?,
        text: payload(args.text, args.text_file).map_err(error)?,
        all: args.all,
        dry_run: args.dry_run,
        check: args.check,
        backup: args.backup,
    };
    let outcome = execute(&args.file, &request).map_err(error)?;
    let summary = crate::output::human::ChangeSummary::edit(&args.file, &request, &outcome);
    Ok(super::common::ok_change(
        &summary,
        &outcome.diff,
        args.max_bytes,
    ))
}
