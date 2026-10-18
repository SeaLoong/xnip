//! Manifest and patch format detection.
//!
//! 优先级：
//! 1. `--format <native|json|compact|yaml|patch>` 显式指定 → 跳过自动识别
//! 2. Patch headers take priority, then recognized file extensions.
//! 3. Without a format hint, leading `[` / `{` selects JSON; otherwise native.
//! 4. Each remaining parser is tried once as a fallback.

use std::path::Path;

use anyhow::Result;

use crate::apply::Op;

/// 显式格式名。
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Format {
    Native,
    Json,
    /// Path-keyed exact pairs or explicit whole-file writes.
    Compact,
    Yaml,
    Patch,
}

/// 解析 `--format` 字符串。
pub fn parse_format_arg(s: &str) -> Result<Format> {
    match s.to_ascii_lowercase().as_str() {
        "native" => Ok(Format::Native),
        "json" => Ok(Format::Json),
        "compact" => Ok(Format::Compact),
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
        Format::Compact => crate::apply::compact::parse(src),
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
        let hint = preferred_from_ext(path);
        if hint == Format::Native && src.trim_start().starts_with(['[', '{']) {
            Format::Json
        } else {
            hint
        }
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
        Format::Json | Format::Compact => {
            [Format::Json, Format::Yaml, Format::Native, Format::Patch]
        }
        Format::Yaml => [Format::Yaml, Format::Json, Format::Native, Format::Patch],
        Format::Native => [Format::Native, Format::Json, Format::Yaml, Format::Patch],
    }
}

#[cfg(test)]
mod tests {
    use super::{Format, parse_auto, parse_with};
    use std::path::Path;

    #[test]
    fn unhinted_json_keeps_its_diagnostic_and_recognized_extensions_keep_priority() {
        for path in [
            None,
            Some(Path::new("manifest")),
            Some(Path::new("manifest.native")),
        ] {
            let error = format!("{:#}", parse_auto(" \n {invalid", path).unwrap_err());
            assert!(error.contains("attempt with Json"), "{error}");
            assert!(!error.contains("attempt with Native"), "{error}");
        }
        let error = format!(
            "{:#}",
            parse_auto(" \n {invalid", Some(Path::new("manifest.yaml"))).unwrap_err()
        );
        assert!(error.contains("attempt with Yaml"), "{error}");
    }

    #[test]
    fn unhinted_json_and_native_inputs_have_the_explicit_parser_semantics() {
        for (source, format) in [
            (" \n {\"a\": [[\"old\", \"new\"]]}", Format::Json),
            ("replace a 1 \"new\"", Format::Native),
        ] {
            assert_eq!(
                format!("{:?}", parse_auto(source, None).unwrap()),
                format!("{:?}", parse_with(source, format).unwrap())
            );
        }
    }
}
