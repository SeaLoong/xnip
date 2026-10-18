//! Shared, concise change summaries for terminal and MCP clients.
use std::fmt::Write as _;
use std::path::Path;

use schemars::JsonSchema;
use serde::Serialize;

use crate::core::edit::{Operation, Outcome, Request};
use crate::core::transaction::Report;

/// Transport presentation; JSON is a single complete result, not a progress stream.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Output {
    #[default]
    Compact,
    Json,
    Human,
}

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
            Operation::Write => "write",
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
            journal: outcome
                .journal
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned()),
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

    fn status(&self) -> &'static str {
        match (self.mode, self.written, self.has_changes) {
            (Mode::Preview, _, _) => "preview",
            (Mode::Check, _, _) => "check",
            (Mode::Write, false, _) => "unchanged",
            (Mode::Write, true, _) => "written",
        }
    }

    /// MCP already carries operation and paths in its call and structured result.
    pub fn mcp_receipt(&self) -> String {
        let mut text = self.status().to_owned();
        if self.mode != Mode::Write {
            text.push_str("; written=false");
        }
        if let Some(journal) = &self.journal {
            let _ = write!(text, "; journal={}", display_path(journal));
        }
        text
    }

    /// A short CLI receipt with an explicit mode, operation and target.
    pub fn compact(&self) -> String {
        let status = self.status();
        let target = if matches!(self.operation.as_str(), "apply" | "recover") {
            format!("{} paths", self.files.len())
        } else {
            self.files.first().map_or_else(String::new, display_path)
        };
        let mut text = format!("{status} {}: {target}", self.operation);
        if let Some(to) = &self.destination {
            let _ = write!(text, " -> {}", display_path(to));
        }
        if let Some(count) = self.replacements {
            let _ = write!(text, " ({count} matches)");
        }
        if self.mode != Mode::Write {
            text.push_str("; written=false");
        }
        if let Some(journal) = &self.journal {
            let _ = write!(text, "; journal={}", display_path(journal));
        }
        text
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

/// Emit a file-change result using the selected CLI presentation.
pub fn emit_change(summary: &ChangeSummary, diff: &str) -> std::io::Result<()> {
    use std::io::Write;
    match crate::output::globals::get().output {
        Output::Json => {
            let mut value = serde_json::to_value(summary).expect("summary serializes");
            if !diff.is_empty() {
                value["diff"] = serde_json::Value::String(diff.to_owned());
            }
            serde_json::to_writer(std::io::stdout().lock(), &value)?;
            writeln!(std::io::stdout().lock())?;
        }
        Output::Human => {
            print_diff(diff)?;
            crate::note!("{}", summary.render());
        }
        Output::Compact => {
            print_diff(&super::budget::truncate_with_hint(
                diff,
                super::budget::DEFAULT_MAX_BYTES,
                "use --output human for full output",
            ))?;
            crate::note!("{}", summary.compact());
        }
    }
    Ok(())
}

/// Legacy line previews retain raw bytes and use the same default output budget.
pub fn print_legacy_preview(bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let max = if crate::output::globals::get().output == Output::Human {
        0
    } else {
        super::budget::DEFAULT_MAX_BYTES
    };
    let selected = super::budget::prefix(bytes, max);
    std::io::stdout().lock().write_all(selected)?;
    if selected.len() < bytes.len() {
        eprintln!(
            "[truncated: {}/{} bytes; use --output human for full output]",
            selected.len(),
            bytes.len()
        );
    }
    Ok(())
}

/// CLI-only: print diff text, with color only on supported terminals.
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
    use super::{ChangeSummary, Mode, display_path};
    use crate::core::edit::{Outcome, Request};
    use std::path::Path;

    #[test]
    fn receipts_distinguish_committed_preview_check_and_noop() {
        for (dry_run, check, changed, expected, written) in [
            (false, false, true, "written replace:", true),
            (true, false, true, "preview replace:", false),
            (false, true, true, "check replace:", false),
            (false, false, false, "unchanged replace:", false),
        ] {
            let request = Request {
                dry_run,
                check,
                ..Default::default()
            };
            let outcome = Outcome {
                changed,
                replacements: 1,
                diff: String::new(),
                journal: None,
            };
            let summary = ChangeSummary::edit(Path::new("文件.rs"), &request, &outcome);
            assert!(summary.compact().starts_with(expected));
            assert_eq!(summary.written, written);
            assert_eq!(summary.has_changes, changed);
            assert_eq!(summary.mode == Mode::Write, !dry_run && !check);
            assert!(!summary.compact().contains('\n'));
            let status = expected.split_whitespace().next().unwrap();
            assert_eq!(
                summary.mcp_receipt(),
                if dry_run || check {
                    format!("{status}; written=false")
                } else {
                    status.into()
                }
            );
        }
    }

    #[test]
    fn mcp_ack_does_not_repeat_counts_available_in_structured_metadata() {
        let request = Request {
            all: true,
            ..Default::default()
        };
        let outcome = Outcome {
            changed: true,
            replacements: 7,
            diff: String::new(),
            journal: None,
        };
        let summary = ChangeSummary::edit(Path::new("a"), &request, &outcome);
        assert_eq!(summary.mcp_receipt(), "written");
        assert_eq!(serde_json::to_value(&summary).unwrap()["replacements"], 7);
        assert!(summary.compact().contains("7 matches"));
        let report = crate::core::transaction::Report {
            files: vec!["a".into(), "b".into()],
            ..Default::default()
        };
        let summary = ChangeSummary::batch("apply", Mode::Write, &report);
        assert_eq!(summary.mcp_receipt(), "written");
        assert_eq!(summary.files.len(), 2);
        assert!(summary.compact().contains("2 paths"));
    }

    #[test]
    fn retained_rename_journal_remains_visible_after_a_successful_write() {
        let request = Request {
            operation: crate::core::edit::Operation::Rename,
            to: Some("b".into()),
            ..Default::default()
        };
        let outcome = Outcome {
            changed: true,
            replacements: 0,
            diff: String::new(),
            journal: Some("retained-journal".into()),
        };
        let summary = ChangeSummary::edit(Path::new("a"), &request, &outcome);
        assert!(summary.written);
        assert!(summary.compact().contains("journal=retained-journal"));
        assert!(summary.mcp_receipt().contains("journal=retained-journal"));
        assert_eq!(
            serde_json::to_value(summary).unwrap()["journal"],
            "retained-journal"
        );
    }

    #[test]
    fn display_paths_cannot_forge_extra_receipt_lines() {
        for path in ["line\nnext", "escape\u{1b}[31m", "quoted\"name", "文件.rs"] {
            let displayed = display_path(path);
            assert!(!displayed.chars().any(char::is_control));
            if displayed.starts_with('"') {
                assert_eq!(serde_json::from_str::<String>(&displayed).unwrap(), path);
            } else {
                assert_eq!(displayed, path);
            }
        }
        assert_eq!(display_path("has spaces.txt"), "\"has spaces.txt\"");
        #[cfg(windows)]
        {
            assert_eq!(display_path(r"\\?\C:\a.rs"), r"C:\a.rs");
            assert_eq!(
                display_path(r"\\?\UNC\server\share\a.rs"),
                r"\\server\share\a.rs"
            );
        }
    }
}
