use rmcp::{ErrorData as McpError, model::CallToolResult};
use schemars::JsonSchema;
use serde::Deserialize;
use std::path::PathBuf;
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Args {
    /// Recovery directory returned by a failed/interrupted transaction.
    pub journal: PathBuf,
    /// Validate and preview rollback without writes.
    #[serde(default)]
    pub dry_run: bool,
}
pub fn run(args: Args) -> Result<CallToolResult, McpError> {
    let report =
        crate::core::transaction::recover(&args.journal, args.dry_run).map_err(|error| {
            McpError::invalid_request(format!("recovery refused/failed: {error:#}"), None)
        })?;
    Ok(super::common::ok_text(format!(
        "xnip_recover: {} {} file(s); journal retained\n{}",
        if args.dry_run {
            "validated"
        } else {
            "restored"
        },
        report.files.len(),
        if args.dry_run {
            report.diff
        } else {
            String::new()
        }
    )))
}
