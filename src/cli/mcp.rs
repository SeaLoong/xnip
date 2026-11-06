//! Stdio MCP adapter with a small default toolset.
use crate::mcp::{Receipts, Toolset};
use crate::output::exit;
use clap::Args as ClapArgs;
use std::path::PathBuf;

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Working directory used to resolve relative tool paths.
    #[arg(long, value_name = "DIRECTORY")]
    pub cwd: Option<PathBuf>,
    /// compact: five everyday tools; full: all specialized tools.
    #[arg(long, value_enum, default_value_t = Toolset::Compact)]
    pub toolset: Toolset,
    /// structured: text and machine metadata; text: only edit/apply/recover receipts and previews.
    #[arg(long, value_enum, default_value_t = Receipts::Structured)]
    pub receipts: Receipts,
}

pub fn run(args: &Args) -> u8 {
    if let Some(cwd) = &args.cwd
        && let Err(error) = std::env::set_current_dir(cwd)
    {
        eprintln!("xnip mcp: working directory {}: {error}", cwd.display());
        return exit::IO;
    }
    crate::trace!("xnip mcp: starting stdio server");
    match crate::mcp::serve_with_options(args.toolset, args.receipts) {
        Ok(()) => exit::SUCCESS,
        Err(error) => {
            eprintln!("xnip mcp: {error:#}");
            exit::IO
        }
    }
}
