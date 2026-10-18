//! YAML 格式 parser。
//!
//! YAML schema 与 JSON 完全相同，仅序列化形式不同。
//! 实现：先用 `serde_saphyr` 解析为 `serde_json::Value`，再委托给 `parse_json::parse`。
//! YAML 与 JSON 因此共享 schema 校验。
#![allow(
    clippy::single_match,
    clippy::single_match_else,
    clippy::needless_raw_string_hashes,
    clippy::wildcard_in_or_patterns
)]

use anyhow::{Context, Result};

/// YAML 解析。
///
/// # Errors
/// YAML 语法错误或转 JSON 失败时返回 `Err`。
pub fn parse(src: &str) -> Result<Vec<crate::apply::Op>> {
    let yaml_value: serde_json::Value =
        serde_saphyr::from_str(src).context("invalid YAML apply manifest")?;
    let json_text = serde_json::to_string(&yaml_value).context("YAML to JSON conversion failed")?;
    crate::apply::parse_json::parse(&json_text)
}
