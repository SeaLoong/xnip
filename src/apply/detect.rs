//! Manifest and patch format detection.
//!
//! 优先级：
//! 1. `--format <native|json|yaml>` 显式指定 → 跳过自动识别
//! 2. 文件后缀强暗示：
//!    - `.json` / `.json5` → 先尝试 JSON
//!    - `.yaml` / `.yml`   → 先尝试 YAML
//!    - 其他 / 无后缀       → 先尝试原生
//! 3. 后缀失败兜底：按 JSON → YAML → 原生 顺序逐个尝试

use std::path::Path;

use anyhow::Result;

use crate::apply::Op;

/// 显式格式名。
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Format {
    Native,
    Json,
    Yaml,
    Patch,
}

/// 解析 `--format` 字符串。
pub fn parse_format_arg(s: &str) -> Result<Format> {
    match s.to_ascii_lowercase().as_str() {
        "native" => Ok(Format::Native),
        "json" => Ok(Format::Json),
        "yaml" | "yml" => Ok(Format::Yaml),
        "patch" | "diff" => Ok(Format::Patch),
        other => anyhow::bail!("unknown --format: {other}"),
    }
}

/// 按指定格式解析。
pub fn parse_with(src: &str, fmt: Format) -> Result<Vec<Op>> {
    match fmt {
        Format::Native => crate::apply::parse_native::parse(src),
        Format::Json => crate::apply::parse_json::parse(src),
        Format::Yaml => crate::apply::parse_yaml::parse(src),
        Format::Patch => crate::apply::patch::parse(src),
    }
}

/// 自动识别格式并解析。`path` 用于后缀提示，可为 `None`（如 stdin 模式）。
pub fn parse_auto(src: &str, path: Option<&Path>) -> Result<Vec<Op>> {
    let preferred = if src.starts_with("*** Begin Patch")
        || src.starts_with("diff --git ")
        || src.starts_with("--- ")
    {
        Format::Patch
    } else {
        preferred_from_ext(path)
    };
    let order = order_for(preferred);

    let mut first_err: Option<anyhow::Error> = None;
    for fmt in order {
        match parse_with(src, fmt) {
            Ok(ops) => return Ok(ops),
            Err(e) => {
                if first_err.is_none() {
                    first_err = Some(e.context(format!("attempt with {fmt:?}")));
                }
            }
        }
    }
    Err(first_err
        .unwrap_or_else(|| anyhow::anyhow!("no parser succeeded"))
        .context("auto-detect failed across native/json/yaml/patch"))
}

fn preferred_from_ext(path: Option<&Path>) -> Format {
    let Some(p) = path else {
        return Format::Native;
    };
    match p
        .extension()
        .and_then(|s| s.to_str())
        .map(str::to_ascii_lowercase)
    {
        Some(ref e) if e == "json" || e == "json5" => Format::Json,
        Some(ref e) if e == "yaml" || e == "yml" => Format::Yaml,
        Some(ref e) if e == "patch" || e == "diff" => Format::Patch,
        _ => Format::Native,
    }
}

fn order_for(preferred: Format) -> [Format; 4] {
    match preferred {
        Format::Patch => [Format::Patch, Format::Json, Format::Yaml, Format::Native],
        Format::Json => [Format::Json, Format::Yaml, Format::Native, Format::Patch],
        Format::Yaml => [Format::Yaml, Format::Json, Format::Native, Format::Patch],
        Format::Native => [Format::Native, Format::Json, Format::Yaml, Format::Patch],
    }
}
