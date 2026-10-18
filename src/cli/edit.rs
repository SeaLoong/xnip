use std::io::Read;
use std::path::PathBuf;

use anyhow::Result;
use clap::Args as ClapArgs;

use crate::core::edit::{EditError, Operation, Placement, Request, execute, payload};
use crate::output::exit;

#[derive(Debug, ClapArgs)]
#[allow(clippy::struct_excessive_bools)]
pub struct Args {
    /// Target path; must exist unless --create or --write is used.
    pub file: PathBuf,
    /// Write the whole file, creating it if missing; old/old-file guards its entire contents.
    #[arg(long, conflicts_with_all = ["create", "delete", "rename_to", "all", "position"])]
    pub write: bool,
    /// Create a missing file, including parent directories; never overwrite.
    #[arg(long, conflicts_with_all = ["delete", "rename_to", "old", "old_file", "all"])]
    pub create: bool,
    /// Delete a regular file; old/old-file optionally checks its entire contents.
    #[arg(long, conflicts_with_all = ["rename_to", "text", "text_file", "text_stdin", "all"])]
    pub delete: bool,
    /// Rename to an absent destination on the same filesystem; never overwrite.
    #[arg(long, conflicts_with_all=["text", "text_file", "text_stdin", "all"])]
    pub rename_to: Option<PathBuf>,
    /// Exact block or anchor; --write/--delete/--rename-to check entire contents.
    #[arg(long, conflicts_with = "old_file")]
    pub old: Option<String>,
    /// Read exact old bytes from a file, including whole-file guards.
    #[arg(long)]
    pub old_file: Option<PathBuf>,
    /// New bytes; empty text deletes the matched block.
    #[arg(long, conflicts_with_all = ["text_file", "text_stdin"])]
    pub text: Option<String>,
    /// Read new bytes from a file, without newline conversion.
    #[arg(long, conflicts_with = "text_stdin")]
    pub text_file: Option<PathBuf>,
    /// Read new bytes from stdin, including an empty stream.
    #[arg(long)]
    pub text_stdin: bool,
    /// Insert new bytes before/after the unique literal old block, without adding newlines.
    #[arg(long, value_enum, conflicts_with_all = ["create", "delete", "rename_to", "write", "all"])]
    pub position: Option<Placement>,
    /// Replace all non-overlapping matches; default requires a unique match.
    #[arg(long)]
    pub all: bool,
    /// Print a diff without changing files or creating directories.
    #[arg(long, conflicts_with = "check")]
    pub dry_run: bool,
    /// Validate without changing files or printing a diff.
    #[arg(long)]
    pub check: bool,
    /// Save original content to <file>.bak before replacement or deletion.
    #[arg(long)]
    pub backup: bool,
}

pub fn run(args: Args) -> Result<u8> {
    let operation = if args.write {
        Operation::Write
    } else if args.create {
        Operation::Create
    } else if args.delete {
        Operation::Delete
    } else if args.rename_to.is_some() {
        Operation::Rename
    } else {
        Operation::Replace
    };
    let mut text = payload(args.text, args.text_file)?;
    if args.text_stdin {
        let mut bytes = Vec::new();
        std::io::stdin().read_to_end(&mut bytes)?;
        text = Some(bytes);
    }
    let request = Request {
        operation,
        to: args.rename_to,
        text,
        old: payload(args.old, args.old_file)?,
        position: args.position,
        all: args.all,
        check: args.check,
        dry_run: args.dry_run,
        backup: args.backup,
    };
    match execute(&args.file, &request) {
        Ok(outcome) => {
            let summary = crate::output::human::ChangeSummary::edit(&args.file, &request, &outcome);
            crate::output::human::emit_change(&summary, &outcome.diff)?;
            Ok(exit::SUCCESS)
        }
        Err(EditError::Check(message)) => {
            eprintln!("xnip edit: {message}");
            Ok(exit::CHECK)
        }
        Err(EditError::Partial(message)) => {
            eprintln!("xnip edit: {message}");
            Ok(exit::PARTIAL)
        }
        Err(error) => Err(error.into()),
    }
}
