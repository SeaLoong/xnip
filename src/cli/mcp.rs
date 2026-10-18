//! Stdio MCP adapter with a small default toolset.
use crate::mcp::{Receipts, Toolset};
use crate::output::exit;
use clap::Args as ClapArgs;

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// compact: five everyday tools; full: all specialized tools.
    #[arg(long, value_enum, default_value_t = Toolset::Compact)]
    pub toolset: Toolset,
    /// structured: text and machine metadata; text: only edit/apply/recover receipts and previews.
    #[arg(long, value_enum, default_value_t = Receipts::Structured)]
    pub receipts: Receipts,
}

pub fn run(args: &Args) -> u8 {
    crate::trace!("xnip mcp: starting stdio server");
    match crate::mcp::serve_with_options(args.toolset, args.receipts) {
        Ok(()) => exit::SUCCESS,
        Err(error) => {
            eprintln!("xnip mcp: {error:#}");
            exit::IO
        }
    }
}
