//! `xnip_doctor` 工具：环境自检。
//!
//! 复用现有 `crate::doctor::report` 把诊断写到 `Vec<u8>` 再回传。

use rmcp::{ErrorData as McpError, model::CallToolResult};

use super::common::{io_to_mcp_err, ok_bytes_as_text};

pub fn run() -> Result<CallToolResult, McpError> {
    let mut buf = Vec::new();
    crate::doctor::report(&mut buf).map_err(io_to_mcp_err)?;
    Ok(ok_bytes_as_text(&buf))
}
