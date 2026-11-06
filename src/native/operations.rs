//! Native wire payloads translated directly to existing core plans and byte
//! operations. No editing semantics or persistence engine lives in this module.
use crate::apply::commit::ExecOpts;
use crate::core::transaction::Plan;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Deserialize)]
#[serde(untagged)]
enum BytePayload {
    Text(String),
    Bytes(Vec<u8>),
}

impl BytePayload {
    fn into_bytes(self) -> Vec<u8> {
        match self {
            Self::Text(text) => text.into_bytes(),
            Self::Bytes(bytes) => bytes,
        }
    }
}

fn payload<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
    BytePayload::deserialize(deserializer).map(BytePayload::into_bytes)
}

fn optional_payload<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Vec<u8>>, D::Error> {
    Option::<BytePayload>::deserialize(deserializer).map(|value| value.map(BytePayload::into_bytes))
}

pub const OP_NAMES: &[&str] = &[
    "exact_replace",
    "insert_exact",
    "write",
    "create",
    "delete",
    "rename",
    "line_replace",
    "line_insert",
    "move",
    "indent",
    "patch",
];

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Position {
    Before,
    After,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(tag = "kind", content = "unit", rename_all = "snake_case")]
pub enum Indent {
    Add(usize),
    Remove(usize),
    TabsToSpaces(usize),
    SpacesToTabs(usize),
}

/// Owned bytes avoid implicit UTF-8 conversion, CRLF conversion and newline
/// synthesis. Paths and tool names are separate concerns: host names never
/// appear in the native operation catalogue.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    ExactReplace {
        path: PathBuf,
        #[serde(deserialize_with = "payload")]
        old: Vec<u8>,
        #[serde(deserialize_with = "payload")]
        text: Vec<u8>,
        #[serde(default)]
        all: bool,
    },
    InsertExact {
        path: PathBuf,
        #[serde(deserialize_with = "payload")]
        anchor: Vec<u8>,
        #[serde(deserialize_with = "payload")]
        text: Vec<u8>,
        position: Position,
    },
    Write {
        path: PathBuf,
        #[serde(deserialize_with = "payload")]
        text: Vec<u8>,
        #[serde(default, deserialize_with = "optional_payload")]
        old: Option<Vec<u8>>,
    },
    Create {
        path: PathBuf,
        #[serde(deserialize_with = "payload")]
        text: Vec<u8>,
    },
    Delete {
        path: PathBuf,
        #[serde(default, deserialize_with = "optional_payload")]
        old: Option<Vec<u8>>,
    },
    Rename {
        path: PathBuf,
        to: PathBuf,
        #[serde(default, deserialize_with = "optional_payload")]
        old: Option<Vec<u8>>,
    },
    LineReplace {
        path: PathBuf,
        start: usize,
        end: usize,
        #[serde(deserialize_with = "payload")]
        text: Vec<u8>,
        #[serde(default, deserialize_with = "optional_payload")]
        was: Option<Vec<u8>>,
    },
    LineInsert {
        path: PathBuf,
        line: usize,
        #[serde(deserialize_with = "payload")]
        text: Vec<u8>,
        position: Position,
    },
    Move {
        path: PathBuf,
        start: usize,
        end: usize,
        to: usize,
        position: Position,
    },
    Indent {
        path: PathBuf,
        start: usize,
        end: usize,
        indent: Indent,
    },
    /// Unified diff or `apply_patch` input, resolved under an explicit root.
    Patch { text: String, root: PathBuf },
}

/// Build a nonwriting core plan in wire-operation order. Original-coordinate
/// grouped manifests keep their existing `apply::commit::prepare` entry point.
pub fn prepare(operations: Vec<Operation>) -> Result<Plan> {
    ensure!(!operations.is_empty(), "operations must not be empty");
    let mut plan = Plan::default();
    for operation in operations {
        stage_operation(&mut plan, operation)?;
    }
    Ok(plan)
}

fn position(value: Position) -> crate::core::ops::insert::Position {
    match value {
        Position::Before => crate::core::ops::insert::Position::Before,
        Position::After => crate::core::ops::insert::Position::After,
    }
}

fn stage_operation(plan: &mut Plan, operation: Operation) -> Result<()> {
    use crate::core::ops::{indent, insert, move_op, replace};
    match operation {
        Operation::ExactReplace {
            path,
            old,
            text,
            all,
        } => {
            plan.replace_exact(&path, &old, &text, all)?;
        }
        Operation::InsertExact {
            path,
            anchor,
            text,
            position,
        } => {
            let placement = match position {
                Position::Before => crate::core::edit::Placement::Before,
                Position::After => crate::core::edit::Placement::After,
            };
            plan.transform(&path, |before| {
                Ok(crate::core::edit::insert_exact(before, &anchor, &text, placement)?.0)
            })?;
        }
        Operation::Write { path, text, old } => plan.write(&path, text, old.as_deref())?,
        Operation::Create { path, text } => plan.create(&path, text)?,
        Operation::Delete { path, old } => plan.delete(&path, old.as_deref())?,
        Operation::Rename { path, to, old } => plan.rename(&path, &to, old.as_deref())?,
        Operation::LineReplace {
            path,
            start,
            end,
            text,
            was,
        } => {
            plan.transform(&path, |before| {
                if let Some(was) = &was {
                    let actual =
                        crate::core::location::extract_line_range_with_newline(before, start, end);
                    anyhow::ensure!(actual == *was, "line content precondition failed");
                }
                Ok(replace::replace_range(before, start, end, &text)?)
            })?;
        }
        Operation::LineInsert {
            path,
            line,
            text,
            position: placement,
        } => {
            plan.transform(&path, |before| {
                Ok(insert::insert_at(before, line, position(placement), &text)?)
            })?;
        }
        Operation::Move {
            path,
            start,
            end,
            to,
            position: placement,
        } => {
            plan.transform(&path, |before| {
                Ok(move_op::move_lines(
                    before,
                    start,
                    end,
                    to,
                    position(placement),
                )?)
            })?;
        }
        Operation::Indent {
            path,
            start,
            end,
            indent: change,
        } => {
            let change = match change {
                Indent::Add(unit) => indent::IndentOp::Add(unit),
                Indent::Remove(unit) => indent::IndentOp::Remove(unit),
                Indent::TabsToSpaces(unit) => indent::IndentOp::TabsToSpaces(unit),
                Indent::SpacesToTabs(unit) => indent::IndentOp::SpacesToTabs(unit),
            };
            plan.transform(&path, |before| {
                Ok(indent::apply_indent(before, start, end, change)?)
            })?;
        }
        Operation::Patch { text, root } => {
            let operations = crate::apply::patch::parse_in(&text, &root)?;
            for operation in operations {
                crate::apply::commit::stage_ordered(plan, &operation, &ExecOpts::default())?;
            }
        }
    }
    Ok(())
}
