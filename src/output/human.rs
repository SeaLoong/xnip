//! Shared, concise change summaries for terminal and MCP clients.
use std::fmt::Write as _;
use std::path::Path;

use schemars::JsonSchema;
use serde::Serialize;

use crate::core::edit::{Operation, Outcome, Request};
use crate::core::transaction::Report;

#[derive(Debug, Clone, Copy, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Write,
    Preview,
    Check,
}

impl Mode {
    pub fn new(dry_run: bool, check: bool) -> Self {
        if dry_run {
            Self::Preview
        } else if check {
            Self::Check
        } else {
            Self::Write
        }
    }
}

/// Machine-readable metadata, without duplicating potentially large diff payloads.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ChangeSummary {
    pub operation: String,
    pub mode: Mode,
    /// Whether planned content or file existence differs from the original.
    pub has_changes: bool,
    /// Whether this successful call committed changes (always false for preview/check).
    pub written: bool,
    /// Target paths; rename includes both source and destination.
    pub files: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destination: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replacements: Option<usize>,
    /// Recovery snapshots retained after the call, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub journal: Option<String>,
}

impl ChangeSummary {
    pub fn edit(file: &Path, request: &Request, outcome: &Outcome) -> Self {
        let operation = match request.operation {
            Operation::Replace => "replace",
            Operation::Create => "create",
            Operation::Delete => "delete",
            Operation::Rename => "rename",
        };
        let mode = Mode::new(request.dry_run, request.check);
        let mut files = vec![file.to_string_lossy().into_owned()];
        if let Some(to) = &request.to {
            files.push(to.to_string_lossy().into_owned());
        }
        Self {
            operation: operation.into(),
            mode,
            has_changes: outcome.changed,
            written: mode == Mode::Write && outcome.changed,
            files,
            destination: request
                .to
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            replacements: (request.operation == Operation::Replace).then_some(outcome.replacements),
            journal: None,
        }
    }

    pub fn batch(operation: &str, mode: Mode, report: &Report) -> Self {
        Self {
            operation: operation.into(),
            mode,
            has_changes: !report.files.is_empty(),
            written: mode == Mode::Write && !report.files.is_empty(),
            files: report
                .files
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect(),
            destination: None,
            replacements: None,
            journal: report
                .journal
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
        }
    }

    /// Plain text works in terminals and clients without Markdown rendering.
    pub fn render(&self) -> String {
        let status = match self.mode {
            Mode::Preview => "Preview (no files written)",
            Mode::Check => "Check passed (no files written)",
            Mode::Write if !self.has_changes => "Unchanged",
            Mode::Write => match self.operation.as_str() {
                "create" => "Created",
                "delete" => "Deleted",
                "rename" => "Renamed",
                "recover" => "Restored",
                _ => "Updated",
            },
        };
        let batch = matches!(self.operation.as_str(), "apply" | "recover");
        let mut text = if batch {
            format!("{status}: {} path(s)", self.files.len())
        } else {
            format!(
                "{status}: {}",
                self.files.first().map_or_else(String::new, display_path)
            )
        };
        if let Some(to) = &self.destination {
            let _ = write!(text, " -> {}", display_path(to));
        }
        if self.mode != Mode::Write {
            let _ = write!(text, "\n  Operation: {}", self.operation);
            if !self.has_changes {
                text.push_str("; no changes needed");
            }
        }
        if let Some(count) = self.replacements {
            let _ = write!(text, "\n  Matched blocks: {count}");
        }
        if batch {
            for file in self.files.iter().take(20) {
                let _ = write!(text, "\n  {}", display_path(file));
            }
            if self.files.len() > 20 {
                let _ = write!(text, "\n  ... {} more path(s)", self.files.len() - 20);
            }
        }
        if let Some(journal) = &self.journal {
            let _ = write!(
                text,
                "\n  Recovery journal retained: {}",
                display_path(journal)
            );
        }
        text
    }
}

/// Quote unusual paths so whitespace/control characters do not forge status lines.
pub fn display_path(path: impl AsRef<Path>) -> String {
    let path = path.as_ref();
    let value = path.to_string_lossy();
    #[cfg(windows)]
    let value = {
        let prefix = r"\\?\";
        let unc_prefix = r"\\?\UNC\";
        if let Some(rest) = value.strip_prefix(unc_prefix) {
            std::borrow::Cow::Owned(format!("{}{rest}", &prefix[..2]))
        } else if let Some(rest) = value.strip_prefix(prefix) {
            std::borrow::Cow::Borrowed(rest)
        } else {
            value
        }
    };
    if value.chars().any(|c| c.is_control() || c == '"') {
        serde_json::to_string(&value).expect("string serialization cannot fail")
    } else if value.contains(' ') {
        format!("\"{value}\"")
    } else {
        value.into_owned()
    }
}

/// CLI-only: keep stdout a complete diff, with color only on supported terminals.
pub fn print_diff(diff: &str) -> std::io::Result<()> {
    use std::io::Write;
    let colored;
    let text = if crate::core::diff::should_colorize_stdout() {
        colored = crate::core::diff::colorize_unified_diff(diff);
        &colored
    } else {
        diff
    };
    std::io::stdout().lock().write_all(text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_paths_preserve_unicode_and_quote_controls() {
        assert_eq!(display_path("src/文件.rs"), "src/文件.rs");
        assert_eq!(display_path("my file.txt"), "\"my file.txt\"");
        let displayed = display_path("line\nnext\u{1b}[31m");
        assert!(!displayed.contains('\n'));
        assert!(!displayed.contains('\u{1b}'));
        assert_eq!(
            serde_json::from_str::<String>(&displayed).unwrap(),
            "line\nnext\u{1b}[31m"
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_display_shortens_extended_prefixes_only_in_prose() {
        assert_eq!(display_path(r"\\?\C:\project\a.rs"), r"C:\project\a.rs");
        assert_eq!(
            display_path(r"\\?\UNC\server\share\a.rs"),
            r"\\server\share\a.rs"
        );
    }
}
