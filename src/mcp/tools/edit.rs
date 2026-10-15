use crate::core::edit::{EditError, Operation, Request, execute, payload};
use rmcp::{ErrorData as McpError, model::CallToolResult};
use schemars::JsonSchema;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(clippy::struct_excessive_bools)]
pub struct Args {
    pub file: PathBuf,
    /// replace (default), create (must not exist), or delete (regular files only).
    #[serde(default)]
    pub operation: Operation,
    /// Exact old block; for delete, optional expected whole-file content.
    pub old: Option<String>,
    pub old_file: Option<PathBuf>,
    /// Replacement/new file bytes. Empty string is allowed.
    pub text: Option<String>,
    pub text_file: Option<PathBuf>,
    #[serde(default)]
    pub all: bool,
    /// Return a diff without writing, including for create/delete.
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub check: bool,
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
        old: payload(args.old, args.old_file).map_err(error)?,
        text: payload(args.text, args.text_file).map_err(error)?,
        all: args.all,
        dry_run: args.dry_run,
        check: args.check,
        backup: args.backup,
    };
    let outcome = execute(&args.file, &request).map_err(error)?;
    Ok(super::common::ok_text(format!(
        "xnip_edit: {} {:?} {} (changed={}, replacements={})\n{}",
        if args.dry_run || args.check {
            "validated"
        } else {
            "completed"
        },
        args.operation,
        args.file.display(),
        outcome.changed,
        outcome.replacements,
        outcome.diff,
    )))
}
