//! Stdio MCP adapter with a small default toolset.
use crate::mcp::Toolset;
use crate::output::exit;
use clap::Args as ClapArgs;

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// compact: five everyday tools; full: all specialized tools.
    #[arg(long, value_enum, default_value_t = Toolset::Compact)]
    pub toolset: Toolset,
}

pub fn run(args: &Args) -> u8 {
    crate::trace!("xnip mcp: starting stdio server");
    match crate::mcp::serve_stdio_with_toolset(args.toolset) {
        Ok(()) => exit::SUCCESS,
        Err(error) => {
            eprintln!("xnip mcp: {error:#}");
            exit::IO
        }
    }
}
