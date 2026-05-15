//! YAML 格式 parser。
//!
//! YAML schema 与 JSON 完全相同（PLAN §6.9.4），仅序列化形式不同。
//! 实现：先用 `serde_yaml` 解析为 `serde_json::Value`，再委托给 `parse_json::parse`。
//! 这样保证 YAML 与 JSON 走同一套 schema 校验，是 PLAN G6 等价性的天然保证。
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
    let yaml_value: serde_yaml::Value =
        serde_yaml::from_str(src).context("invalid YAML apply manifest")?;
    let json_text = serde_json::to_string(&yaml_value).context("YAML to JSON conversion failed")?;
    crate::apply::parse_json::parse(&json_text)
}

#[cfg(test)]
#[allow(clippy::single_match)]
mod tests {
    use super::*;
    use crate::apply::{Op, OpContent, Target};
    use crate::core::location::Locator;
    use std::path::PathBuf;

    const YAML_SAMPLE: &str = r#"
- op: replace
  file: src/Foo.vue
  lines: "30-45"
  text: |
    function foo() {
      return 42;
    }

- op: insert
  file: src/Foo.vue
  match-line: "^import vue"
  where: after
  text: "import { ref } from 'vue';"
"#;

    #[test]
    fn parses_two_ops() {
        let ops = parse(YAML_SAMPLE).unwrap();
        assert_eq!(ops.len(), 2);
    }

    #[test]
    fn first_op_is_replace_with_block_text() {
        let ops = parse(YAML_SAMPLE).unwrap();
        match &ops[0] {
            Op::Replace {
                target,
                locator,
                content,
                ..
            } => {
                assert!(matches!(target, Target::File(p) if p == &PathBuf::from("src/Foo.vue")));
                assert!(matches!(locator, Locator::Lines { start: 30, end: 45 }));
                match content {
                    OpContent::Text(b) => {
                        let s = std::str::from_utf8(b).unwrap();
                        assert!(s.starts_with("function foo()"));
                        assert!(s.contains("return 42"));
                    }
                    _ => panic!(),
                }
            }
            _ => panic!(),
        }
    }

    #[test]
    fn second_op_is_insert_match_line() {
        let ops = parse(YAML_SAMPLE).unwrap();
        match &ops[1] {
            Op::Insert {
                locator, content, ..
            } => {
                assert!(matches!(locator, Locator::MatchLine { .. }));
                assert!(matches!(content, OpContent::Text(_)));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn invalid_yaml_errors() {
        let bad = "this: is: not: valid: yaml: [\n";
        assert!(parse(bad).is_err());
    }

    #[test]
    fn missing_target_errors() {
        // YAML schema 与 JSON 一致；missing `file` 必报错
        let src = "- op: replace\n  lines: \"1\"\n  text: x\n";
        assert!(parse(src).is_err());
    }

    #[test]
    fn replace_pattern_repl() {
        let src = r#"
- op: replace
  file: a.txt
  pattern: OLD
  repl: NEW
  count: all
"#;
        let ops = parse(src).unwrap();
        match &ops[0] {
            Op::Replace {
                locator, content, ..
            } => {
                assert!(matches!(locator, Locator::Pattern { .. }));
                assert!(matches!(content, OpContent::Repl(s) if s == "NEW"));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn empty_yaml_array_yields_empty_ops() {
        let ops = parse("[]").unwrap();
        assert!(ops.is_empty());
    }

    #[test]
    fn move_and_indent_round_trip_via_json() {
        let src = r#"
- op: move
  file: a
  lines: "10-20"
  to: 100
- op: indent
  file: a
  lines: "1-9"
  by: 2
"#;
        let ops = parse(src).unwrap();
        assert_eq!(ops.len(), 2);
        assert!(matches!(ops[0], Op::Move { to_line: 100, .. }));
        assert!(matches!(
            ops[1],
            Op::Indent {
                kind: crate::apply::IndentKind::Adjust(2),
                ..
            }
        ));
    }
}
