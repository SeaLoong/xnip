//! Shared file creation, exact block replacement and guarded deletion.
use std::fs;
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::Deserialize;
use thiserror::Error;

use super::atomic;

#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    #[default]
    Replace,
    Create,
    Delete,
    Rename,
}

#[derive(Debug, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct Request {
    pub operation: Operation,
    pub to: Option<PathBuf>,
    pub old: Option<Vec<u8>>,
    pub text: Option<Vec<u8>>,
    pub all: bool,
    pub dry_run: bool,
    pub check: bool,
    pub backup: bool,
}

#[derive(Debug)]
pub struct Outcome {
    pub changed: bool,
    pub replacements: usize,
    pub diff: String,
    /// Recovery snapshots retained if successful rename cleanup failed.
    pub journal: Option<PathBuf>,
}

#[derive(Debug, Error)]
pub enum EditError {
    #[error("{0}")]
    Usage(&'static str),
    #[error("edit precondition failed: {0}")]
    Check(String),
    #[error("{0}")]
    Partial(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Atomic(#[from] atomic::AtomicError),
}

/// Load an optional byte payload without shell escape or newline conversion.
pub fn payload(text: Option<String>, file: Option<PathBuf>) -> Result<Option<Vec<u8>>, EditError> {
    match (text, file) {
        (Some(_), Some(_)) => Err(EditError::Usage(
            "inline text and a payload file are mutually exclusive",
        )),
        (Some(text), None) => Ok(Some(text.into_bytes())),
        (None, Some(path)) => Ok(Some(fs::read(path)?)),
        (None, None) => Ok(None),
    }
}

/// Execute one file operation. Preview/check modes do not create directories.
pub fn execute(path: &Path, request: &Request) -> Result<Outcome, EditError> {
    if request.check && request.dry_run {
        return Err(EditError::Usage("check and dry_run are mutually exclusive"));
    }
    if request.operation == Operation::Rename {
        if request.text.is_some() || request.all {
            return Err(EditError::Usage(
                "rename does not accept replacement text or all",
            ));
        }
        let to = request
            .to
            .as_deref()
            .ok_or(EditError::Usage("rename requires a destination"))?;
        let mut plan = super::transaction::Plan::default();
        plan.rename(path, to, request.old.as_deref())
            .map_err(transaction_error)?;
        let preview = if request.dry_run {
            plan.preview()
        } else {
            super::transaction::Report {
                files: plan.changed_files(),
                ..super::transaction::Report::default()
            }
        };
        let journal = if !request.check && !request.dry_run {
            plan.commit(request.backup)
                .map_err(transaction_error)?
                .journal
        } else {
            None
        };
        return Ok(Outcome {
            journal,
            changed: !preview.files.is_empty(),
            replacements: 0,
            diff: if request.dry_run {
                preview.diff
            } else {
                String::new()
            },
        });
    }
    if request.to.is_some() {
        return Err(EditError::Usage("destination is only valid for rename"));
    }
    let (before, after, replacements, target) = match request.operation {
        Operation::Rename => unreachable!("rename handled above"),
        Operation::Create => {
            if request.old.is_some() || request.all {
                return Err(EditError::Usage("create does not accept old text or all"));
            }
            let text = request
                .text
                .as_ref()
                .ok_or(EditError::Usage("create requires text or a text file"))?;
            match fs::symlink_metadata(path) {
                Ok(_) => return Err(EditError::Check("create target already exists".into())),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
            (Vec::new(), text.clone(), 0, path.to_path_buf())
        }
        Operation::Replace | Operation::Delete => {
            let metadata = fs::symlink_metadata(path)?;
            if request.operation == Operation::Delete && metadata.file_type().is_symlink() {
                return Err(EditError::Usage("delete does not follow symbolic links"));
            }
            let target = fs::canonicalize(path)?;
            if !fs::metadata(&target)?.is_file() {
                return Err(EditError::Usage("edit target must be a regular file"));
            }
            let before = fs::read(&target)?;
            let (after, count) = if request.operation == Operation::Delete {
                if request.text.is_some() || request.all {
                    return Err(EditError::Usage(
                        "delete does not accept replacement text or all",
                    ));
                }
                if request
                    .old
                    .as_ref()
                    .is_some_and(|expected| expected != &before)
                {
                    return Err(EditError::Check(
                        "file does not equal expected old content".into(),
                    ));
                }
                (Vec::new(), 0)
            } else {
                let old = request
                    .old
                    .as_deref()
                    .ok_or(EditError::Usage("replace requires old text or an old file"))?;
                let new = request.text.as_deref().ok_or(EditError::Usage(
                    "replace requires text or a text file; empty text deletes the block",
                ))?;
                replace_exact(&before, old, new, request.all)?
            };
            (before, after, count, target)
        }
    };
    let changed = request.operation != Operation::Replace || before != after;
    let diff = if request.dry_run {
        super::diff::unified_diff(path, &before, &after)
    } else {
        String::new()
    };
    if changed && !request.check && !request.dry_run {
        if request.operation == Operation::Create {
            let parent = target
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            fs::create_dir_all(parent)?;
            // No-clobber persistence is essential even if another writer creates
            // the file after the initial absence check.
            atomic::prepare_write(&target, &after)?
                .persist_noclobber(&target)
                .map_err(|e| EditError::Io(e.error))?;
        } else {
            if fs::read(&target)? != before {
                return Err(EditError::Check(
                    "file changed while preparing the edit".into(),
                ));
            }
            if request.operation == Operation::Delete {
                if request.backup {
                    atomic::backup(&target)?;
                }
                fs::remove_file(&target)?;
            } else {
                atomic::atomic_write(&target, &after, request.backup)?;
            }
        }
    }
    Ok(Outcome {
        changed,
        replacements,
        diff,
        journal: None,
    })
}

/// Literal matching preserves all bytes. Ambiguous (including overlapping)
/// matches fail unless the caller explicitly chooses all non-overlapping hits.
pub fn replace_exact(
    content: &[u8],
    old: &[u8],
    new: &[u8],
    all: bool,
) -> Result<(Vec<u8>, usize), EditError> {
    if old.is_empty() {
        return Err(EditError::Usage(
            "old text must not be empty; use insert or create",
        ));
    }
    let first = memchr::memmem::find(content, old).ok_or_else(|| {
        EditError::Check("old text was not found; include exact whitespace and line endings".into())
    })?;
    if !all && memchr::memmem::find(&content[first + 1..], old).is_some() {
        return Err(EditError::Check(
            "old text is ambiguous; include more context or explicitly request all matches".into(),
        ));
    }
    let mut output = Vec::with_capacity(content.len());
    let mut cursor = 0;
    let mut count = 0;
    let mut found = Some(first);
    while let Some(start) = found {
        output.extend_from_slice(&content[cursor..start]);
        output.extend_from_slice(new);
        cursor = start + old.len();
        count += 1;
        found = if all {
            memchr::memmem::find(&content[cursor..], old).map(|offset| cursor + offset)
        } else {
            None
        };
    }
    output.extend_from_slice(&content[cursor..]);
    Ok((output, count))
}

fn transaction_error(error: anyhow::Error) -> EditError {
    if error
        .downcast_ref::<super::transaction::CommitError>()
        .is_some()
    {
        EditError::Partial(format!("{error:#}"))
    } else if error
        .chain()
        .any(<dyn std::error::Error>::is::<std::io::Error>)
    {
        EditError::Io(std::io::Error::other(error))
    } else {
        EditError::Check(format!("{error:#}"))
    }
}
