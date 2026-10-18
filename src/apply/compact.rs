//! File-grouped literal edits and explicit whole-file writes.
//! Each file is independent; pairs within one file execute in order.
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use schemars::JsonSchema;
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer};

use super::{FileOp, Op, OpContent, Target};

/// A path maps to exact pairs, a complete new file, or a guarded complete file.
#[derive(Debug, JsonSchema)]
#[schemars(transparent)]
pub struct Files(pub BTreeMap<String, FileEdits>);

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum FileEdits {
    Exact(Vec<[String; 2]>),
    Write(String),
    Guarded(WholeWrite),
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WholeWrite {
    pub text: String,
    #[serde(default)]
    pub old: Option<String>,
}

impl<'de> Deserialize<'de> for Files {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct FilesVisitor;
        impl<'de> Visitor<'de> for FilesVisitor {
            type Value = Files;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a map of paths to exact pairs or complete file contents")
            }
            fn visit_map<M: MapAccess<'de>>(
                self,
                mut map: M,
            ) -> std::result::Result<Files, M::Error> {
                let mut files = BTreeMap::new();
                while let Some((path, edits)) = map.next_entry::<String, FileEdits>()? {
                    if files.insert(path.clone(), edits).is_some() {
                        return Err(serde::de::Error::custom(format!(
                            "duplicate file key: {path}"
                        )));
                    }
                }
                Ok(Files(files))
            }
        }
        deserializer.deserialize_map(FilesVisitor)
    }
}

impl Files {
    /// Compile independent file groups to the shared transactional instruction set.
    ///
    /// # Errors
    /// Rejects empty groups, ambiguous target aliases, and empty exact old blocks.
    pub fn into_ops(self) -> Result<Vec<Op>> {
        if self.0.is_empty() {
            bail!("files must contain at least one file");
        }
        let mut targets = BTreeSet::new();
        let mut ops = Vec::new();
        for (path, edits) in self.0 {
            if path.is_empty() {
                bail!("file path must not be empty");
            }
            let path = PathBuf::from(path);
            let canonical = crate::core::transaction::normalize(&path)
                .with_context(|| format!("resolve {}", path.display()))?;
            let identity = crate::core::transaction::path_identity(&canonical);
            if !targets.insert(identity) {
                bail!(
                    "files contains aliases of the same target: {}",
                    path.display()
                );
            }
            let target = || Target::File(path.clone());
            match edits {
                FileEdits::Exact(pairs) => {
                    if pairs.is_empty() {
                        bail!(
                            "file group must contain at least one pair: {}",
                            path.display()
                        );
                    }
                    for [old, text] in pairs {
                        if old.is_empty() {
                            bail!("exact old block must not be empty: {}", path.display());
                        }
                        ops.push(Op::File {
                            target: target(),
                            operation: FileOp::Exact {
                                old: OpContent::Text(old.into_bytes()),
                                text: OpContent::Text(text.into_bytes()),
                                all: false,
                            },
                        });
                    }
                }
                FileEdits::Write(text) => ops.push(Op::File {
                    target: target(),
                    operation: FileOp::Write {
                        content: OpContent::Text(text.into_bytes()),
                        was: None,
                    },
                }),
                FileEdits::Guarded(WholeWrite { text, old }) => ops.push(Op::File {
                    target: target(),
                    operation: FileOp::Write {
                        content: OpContent::Text(text.into_bytes()),
                        was: old.map(String::into_bytes),
                    },
                }),
            }
        }
        Ok(ops)
    }
}

/// Parse the compact path map. Duplicate JSON keys are rejected.
///
/// # Errors
/// Returns an error for invalid shapes, duplicate targets, or empty groups.
pub fn parse(src: &str) -> Result<Vec<Op>> {
    serde_json::from_str::<Files>(src)
        .context("invalid compact apply manifest")?
        .into_ops()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_shape_and_no_duplicate_keys() {
        for src in [
            "{}",
            r#"{"a":[]}"#,
            r#"{"a":[["","x"]]}"#,
            r#"{"a":[["x"]]}"#,
            r#"{"a":[["x","y","z"]]}"#,
            r#"{"a":null}"#,
            r#"{"a":{"text":"x","all":true}}"#,
            r#"{"a":"one","a":"two"}"#,
            r#"{"a":"one","./a":"two"}"#,
        ] {
            assert!(parse(src).is_err(), "accepted {src}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn unicode_case_aliases_are_rejected_but_one_to_many_case_expansions_stay_distinct() {
        let directory = tempfile::tempdir().unwrap();
        for (left, right) in [("Ä", "ä"), ("Σ", "σ")] {
            let source = serde_json::json!({
                directory.path().join(format!("{left}.txt")).to_str().unwrap(): "one",
                directory.path().join(format!("{right}.txt")).to_str().unwrap(): "two"
            })
            .to_string();
            assert!(parse(&source).is_err(), "accepted aliases {left}/{right}");
        }
        for (left, right) in [("Σ", "ς"), ("ſ", "S"), ("ß", "SS"), ("\u{212a}", "k")] {
            let source = serde_json::json!({
                directory.path().join(format!("{left}.txt")).to_str().unwrap(): "one",
                directory.path().join(format!("{right}.txt")).to_str().unwrap(): "two"
            })
            .to_string();
            assert_eq!(parse(&source).unwrap().len(), 2, "conflated {left}/{right}");
        }
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    }

    #[test]
    fn whole_content_and_pairs_are_distinct() {
        let ops =
            parse(r#"{"a":[["old","new"],["new","next"]],"b":"","c":{"old":"was","text":"now"}}"#)
                .unwrap();
        assert_eq!(ops.len(), 4);
        assert!(matches!(
            &ops[2],
            Op::File {
                operation: FileOp::Write { was: None, .. },
                ..
            }
        ));
        assert!(
            matches!(&ops[3], Op::File { operation: FileOp::Write { was: Some(was), .. }, .. } if was == b"was")
        );
    }
}
