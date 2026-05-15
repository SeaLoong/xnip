//! JSON 格式 parser。
//!
//! 顶层是 op 数组。schema 见 PLAN §6.9.3：
//!
//! ```json
//! [
//!   {"op": "replace", "file": "a.txt", "lines": "30", "text": "X"},
//!   {"op": "replace", "files-from": "list.txt",
//!    "pattern": "OLD", "repl": "NEW", "count": "all"},
//!   {"op": "insert", "file": "a.txt", "lines": 5, "where": "after", "text": "X"},
//!   {"op": "move", "file": "a.txt", "lines": "10-20", "to": 100},
//!   {"op": "indent", "file": "a.txt", "lines": "30-45", "by": 2}
//! ]
//! ```
#![allow(
    clippy::single_match,
    clippy::single_match_else,
    clippy::wildcard_in_or_patterns,
    clippy::redundant_closure,
    clippy::wrong_self_convention,
    clippy::unnecessary_wraps
)]

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use regex::Regex;
use regex::bytes::Regex as ByteRegex;
use serde::Deserialize;
use serde_json::Value;

use crate::apply::{IndentKind, MovePosition, Op, OpContent, Target};
use crate::core::location::{Count, Locator};

/// JSON 数组顶层解析。
///
/// # Errors
/// 当 JSON 语法错误、缺字段、字段类型不对时返回 `Err`。
pub fn parse(src: &str) -> Result<Vec<Op>> {
    let arr: Vec<RawOp> = serde_json::from_str(src).context("invalid JSON apply manifest")?;
    arr.into_iter()
        .enumerate()
        .map(|(i, raw)| raw.into_op().with_context(|| format!("op #{i}")))
        .collect()
}

/// 中间结构：与 JSON schema 一一对应；松散接受多种字段写法。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct RawOp {
    op: String,

    // target
    #[serde(default)]
    file: Option<String>,
    #[serde(default)]
    files_from: Option<String>,

    // locator
    #[serde(default)]
    lines: Option<Value>, // 字符串 "30" / "30-45"，或 number 30
    #[serde(default)]
    match_line: Option<String>,
    #[serde(default)]
    occurrence: Option<usize>,
    #[serde(default)]
    between: Option<Vec<String>>,
    #[serde(default)]
    between_re: Option<Vec<String>>,
    #[serde(default)]
    inclusive: Option<bool>,
    #[serde(default)]
    pattern: Option<String>,
    #[serde(default)]
    count: Option<Value>, // "all" 或 number

    // content
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    text_file: Option<String>,
    #[serde(default)]
    repl: Option<String>,

    // modifiers
    #[serde(default, rename = "where")]
    where_: Option<String>,
    #[serde(default)]
    to: Option<usize>,
    #[serde(default)]
    by: Option<i64>,
    #[serde(default)]
    tabs_to_spaces: Option<usize>,
    #[serde(default)]
    spaces_to_tabs: Option<usize>,
    #[serde(default)]
    revert: Option<bool>,
    #[serde(default)]
    was: Option<String>,
    #[serde(default)]
    was_file: Option<String>,
}

impl RawOp {
    fn into_op(self) -> Result<Op> {
        let op_name = self.op.clone();
        match op_name.as_str() {
            "replace" => self.to_replace(),
            "insert" => self.to_insert(),
            "move" => self.to_move(),
            "indent" => self.to_indent(),
            other => bail!("unknown op: {other}"),
        }
    }

    fn target(&self) -> Result<Target> {
        match (&self.file, &self.files_from) {
            (Some(f), None) => Ok(Target::File(PathBuf::from(f))),
            (None, Some(l)) => Ok(Target::FilesFrom(PathBuf::from(l))),
            (Some(_), Some(_)) => bail!("`file` and `files-from` are mutually exclusive"),
            (None, None) => bail!("missing `file` or `files-from`"),
        }
    }

    fn locator(&self) -> Result<Locator> {
        let mut count_kinds = 0;
        if self.lines.is_some() {
            count_kinds += 1;
        }
        if self.match_line.is_some() {
            count_kinds += 1;
        }
        if self.between.is_some() {
            count_kinds += 1;
        }
        if self.between_re.is_some() {
            count_kinds += 1;
        }
        if self.pattern.is_some() {
            count_kinds += 1;
        }
        if count_kinds == 0 {
            bail!("missing locator");
        }
        if count_kinds > 1 {
            bail!("conflicting locators");
        }

        if let Some(v) = &self.lines {
            let (s, e) = parse_lines_value(v)?;
            return Ok(Locator::Lines { start: s, end: e });
        }
        if let Some(re) = &self.match_line {
            return Ok(Locator::MatchLine {
                regex: Regex::new(re).with_context(|| format!("invalid match-line: {re}"))?,
                occurrence: self.occurrence.unwrap_or(1).max(1),
            });
        }
        if let Some(arr) = &self.between {
            if arr.len() != 2 {
                bail!("`between` must be a 2-element array");
            }
            return Ok(Locator::Between {
                start: arr[0].as_bytes().to_vec(),
                end: arr[1].as_bytes().to_vec(),
                start_occ: self.occurrence.unwrap_or(1).max(1),
                end_occ: 1,
                inclusive: self.inclusive.unwrap_or(false),
            });
        }
        if let Some(arr) = &self.between_re {
            if arr.len() != 2 {
                bail!("`between-re` must be a 2-element array");
            }
            return Ok(Locator::BetweenRe {
                start: Regex::new(&arr[0])
                    .with_context(|| format!("invalid between-re start: {}", arr[0]))?,
                end: Regex::new(&arr[1])
                    .with_context(|| format!("invalid between-re end: {}", arr[1]))?,
                start_occ: self.occurrence.unwrap_or(1).max(1),
                end_occ: 1,
                inclusive: self.inclusive.unwrap_or(false),
            });
        }
        if let Some(p) = &self.pattern {
            let cnt = parse_count_value(self.count.as_ref())?;
            return Ok(Locator::Pattern {
                regex: ByteRegex::new(p).with_context(|| format!("invalid pattern: {p}"))?,
                count: cnt,
            });
        }
        unreachable!()
    }

    fn content(&self, allow_repl: bool) -> Result<OpContent> {
        let count = [
            self.text.is_some(),
            self.text_file.is_some(),
            self.repl.is_some(),
        ]
        .iter()
        .filter(|x| **x)
        .count();
        if count > 1 {
            bail!("`text`, `text-file`, `repl` are mutually exclusive");
        }
        if let Some(s) = &self.text {
            if s.is_empty() {
                return Ok(OpContent::Empty);
            }
            return Ok(OpContent::Text(s.clone().into_bytes()));
        }
        if let Some(p) = &self.text_file {
            return Ok(OpContent::File(PathBuf::from(p)));
        }
        if let Some(r) = &self.repl {
            if !allow_repl {
                bail!("`repl` is only valid with `pattern` locator");
            }
            return Ok(OpContent::Repl(r.clone()));
        }
        Ok(OpContent::None)
    }

    fn position(&self) -> MovePosition {
        match self.where_.as_deref() {
            Some("before") => MovePosition::Before,
            _ => MovePosition::After,
        }
    }

    fn was_bytes(&self) -> Result<Option<Vec<u8>>> {
        match (&self.was, &self.was_file) {
            (Some(_), Some(_)) => bail!("`was` and `was-file` are mutually exclusive"),
            (Some(s), None) => Ok(Some(s.clone().into_bytes())),
            (None, Some(p)) => {
                Ok(Some(std::fs::read(p).with_context(|| {
                    format!("failed to read was-file: {p}")
                })?))
            }
            (None, None) => Ok(None),
        }
    }

    fn to_replace(self) -> Result<Op> {
        let target = self.target()?;
        let locator = self.locator()?;
        let revert = self.revert.unwrap_or(false);
        let allow_repl = matches!(locator, Locator::Pattern { .. });
        let content = self.content(allow_repl)?;
        if matches!(content, OpContent::None) {
            bail!("replace requires content (`text` / `text-file` / `repl`)");
        }
        let was = self.was_bytes()?;
        Ok(Op::Replace {
            target,
            locator,
            content,
            was,
            revert,
        })
    }

    fn to_insert(self) -> Result<Op> {
        let target = self.target()?;
        let locator = self.locator()?;
        let position = self.position();
        let content = self.content(false)?;
        if matches!(content, OpContent::None) {
            bail!("insert requires content");
        }
        Ok(Op::Insert {
            target,
            locator,
            content,
            position,
            revert: self.revert.unwrap_or(false),
        })
    }

    fn to_move(self) -> Result<Op> {
        let target = self.target()?;
        let from = self.locator()?;
        let to = self.to.context("move requires `to` line number")?;
        let position = self.position();
        Ok(Op::Move {
            target,
            from,
            to_line: to,
            position,
            revert: self.revert.unwrap_or(false),
        })
    }

    fn to_indent(self) -> Result<Op> {
        let target = self.target()?;
        let locator = self.locator()?;
        let kind = match (self.by, self.tabs_to_spaces, self.spaces_to_tabs) {
            (Some(n), None, None) => IndentKind::Adjust(n),
            (None, Some(n), None) => IndentKind::TabsToSpaces(n),
            (None, None, Some(n)) => IndentKind::SpacesToTabs(n),
            (None, None, None) => {
                bail!("indent requires one of `by`/`tabs-to-spaces`/`spaces-to-tabs`")
            }
            _ => bail!("indent ops are mutually exclusive"),
        };
        Ok(Op::Indent {
            target,
            locator,
            kind,
            revert: self.revert.unwrap_or(false),
        })
    }
}

fn parse_lines_value(v: &Value) -> Result<(usize, usize)> {
    match v {
        Value::String(s) => parse_lines_str(s),
        Value::Number(n) => {
            let n = n
                .as_u64()
                .context("`lines` number must be non-negative integer")?
                as usize;
            Ok((n, n))
        }
        _ => bail!("`lines` must be string or integer"),
    }
}

fn parse_lines_str(s: &str) -> Result<(usize, usize)> {
    if let Some((a, b)) = s.split_once('-') {
        Ok((
            a.trim()
                .parse::<usize>()
                .with_context(|| format!("invalid start in lines: {a:?}"))?,
            b.trim()
                .parse::<usize>()
                .with_context(|| format!("invalid end in lines: {b:?}"))?,
        ))
    } else {
        let n = s
            .trim()
            .parse::<usize>()
            .with_context(|| format!("invalid lines: {s:?}"))?;
        Ok((n, n))
    }
}

fn parse_count_value(v: Option<&Value>) -> Result<Count> {
    match v {
        None => Ok(Count::All),
        Some(Value::String(s)) if s.eq_ignore_ascii_case("all") => Ok(Count::All),
        Some(Value::String(s)) => {
            let n = s
                .parse::<usize>()
                .with_context(|| format!("invalid count: {s:?}"))?;
            if n == 0 {
                bail!("count must be >= 1, or \"all\"");
            }
            Ok(Count::First(n))
        }
        Some(Value::Number(n)) => {
            let n = n.as_u64().context("count number must be non-negative")?;
            if n == 0 {
                bail!("count must be >= 1");
            }
            Ok(Count::First(n as usize))
        }
        Some(_) => bail!("count must be string or integer"),
    }
}

#[cfg(test)]
#[allow(clippy::single_match)]
mod tests {
    use super::*;

    #[test]
    fn replace_lines_with_text() {
        let ops = parse(r#"[{"op":"replace","file":"a.txt","lines":"30","text":"X"}]"#).unwrap();
        assert_eq!(ops.len(), 1);
        match &ops[0] {
            Op::Replace {
                target,
                locator,
                content,
                ..
            } => {
                assert!(matches!(target, Target::File(p) if p == &PathBuf::from("a.txt")));
                assert!(matches!(locator, Locator::Lines { start: 30, end: 30 }));
                assert!(matches!(content, OpContent::Text(b) if b == b"X"));
            }
            _ => panic!("expected Replace"),
        }
    }

    #[test]
    fn replace_lines_range_string() {
        let ops = parse(r#"[{"op":"replace","file":"a.txt","lines":"30-45","text":""}]"#).unwrap();
        match &ops[0] {
            Op::Replace {
                locator, content, ..
            } => {
                assert!(matches!(locator, Locator::Lines { start: 30, end: 45 }));
                assert!(matches!(content, OpContent::Empty));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn replace_lines_number() {
        let ops = parse(r#"[{"op":"replace","file":"a.txt","lines":5,"text":"X"}]"#).unwrap();
        match &ops[0] {
            Op::Replace { locator, .. } => {
                assert!(matches!(locator, Locator::Lines { start: 5, end: 5 }));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn replace_pattern_repl() {
        let ops =
            parse(r#"[{"op":"replace","file":"a","pattern":"OLD","repl":"NEW","count":"all"}]"#)
                .unwrap();
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
    fn replace_pattern_count_number() {
        let ops =
            parse(r#"[{"op":"replace","file":"a","pattern":"x","repl":"y","count":3}]"#).unwrap();
        if let Op::Replace { locator, .. } = &ops[0]
            && let Locator::Pattern { count, .. } = locator
        {
            assert_eq!(*count, Count::First(3));
        } else {
            panic!();
        }
    }

    #[test]
    fn insert_with_where_after() {
        let ops =
            parse(r#"[{"op":"insert","file":"a","lines":5,"where":"after","text":"X"}]"#).unwrap();
        match &ops[0] {
            Op::Insert { position, .. } => {
                assert!(matches!(position, MovePosition::After));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn move_op_basic() {
        let ops = parse(r#"[{"op":"move","file":"a","lines":"10-20","to":100}]"#).unwrap();
        match &ops[0] {
            Op::Move {
                from,
                to_line,
                position,
                ..
            } => {
                assert!(matches!(from, Locator::Lines { start: 10, end: 20 }));
                assert_eq!(*to_line, 100);
                assert!(matches!(position, MovePosition::After));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn indent_by() {
        let ops = parse(r#"[{"op":"indent","file":"a","lines":"1-9","by":2}]"#).unwrap();
        match &ops[0] {
            Op::Indent { kind, .. } => assert!(matches!(kind, IndentKind::Adjust(2))),
            _ => panic!(),
        }
    }

    #[test]
    fn indent_tabs_to_spaces() {
        let ops =
            parse(r#"[{"op":"indent","file":"a","lines":"1-9","tabs-to-spaces":4}]"#).unwrap();
        match &ops[0] {
            Op::Indent { kind, .. } => {
                assert!(matches!(kind, IndentKind::TabsToSpaces(4)));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn between_two_element_array() {
        let ops =
            parse(r#"[{"op":"replace","file":"a","between":["BEGIN","END"],"text":""}]"#).unwrap();
        if let Op::Replace { locator, .. } = &ops[0]
            && let Locator::Between { start, end, .. } = locator
        {
            assert_eq!(start, b"BEGIN");
            assert_eq!(end, b"END");
        } else {
            panic!();
        }
    }

    #[test]
    fn was_field_supported() {
        let ops = parse(r#"[{"op":"replace","file":"a","lines":"3","text":"new","was":"old\n"}]"#)
            .unwrap();
        match &ops[0] {
            Op::Replace { was, .. } => assert_eq!(was.as_deref(), Some(b"old\n".as_ref())),
            _ => panic!(),
        }
    }

    #[test]
    fn revert_field_supported() {
        let ops = parse(r#"[{"op":"replace","file":"a","pattern":"x","repl":"y","revert":true}]"#)
            .unwrap();
        match &ops[0] {
            Op::Replace { revert, .. } => assert!(*revert),
            _ => panic!(),
        }
    }

    #[test]
    fn unknown_op_errors() {
        assert!(parse(r#"[{"op":"unknown","file":"a"}]"#).is_err());
    }

    #[test]
    fn missing_target_errors() {
        assert!(parse(r#"[{"op":"replace","lines":"1","text":"x"}]"#).is_err());
    }

    #[test]
    fn conflicting_locators_errors() {
        assert!(
            parse(r#"[{"op":"replace","file":"a","lines":"1","match-line":"x","text":"y"}]"#,)
                .is_err()
        );
    }

    #[test]
    fn empty_array_yields_empty_ops() {
        let ops = parse("[]").unwrap();
        assert!(ops.is_empty());
    }

    #[test]
    fn invalid_json_errors() {
        assert!(parse("not json").is_err());
    }
}
