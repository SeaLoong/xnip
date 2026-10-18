//! CLI adapter for bounded numbered reads.

use std::io::Write;
use std::path::PathBuf;

use anyhow::Result;
use clap::Args as ClapArgs;

use crate::core::content;
use crate::core::ops::peek::PeekError;
use crate::core::read::PeekQuery;
use crate::output::exit;

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// File to read.
    pub file: PathBuf,
    /// 1-based line or inclusive range.
    #[arg(long, value_name = "RANGE")]
    pub lines: Option<String>,
    /// Line regex, optionally with context.
    #[arg(long, value_name = "REGEX")]
    pub match_line: Option<String>,
    /// Context on each side of matching lines.
    #[arg(long, default_value_t = 0, value_name = "N")]
    pub context: usize,
    /// Read from the beginning.
    #[arg(long)]
    pub all: bool,
    /// Maximum lines (default 200; 0 unlimited).
    #[arg(long, value_name = "N")]
    pub max_lines: Option<usize>,
    /// Maximum output bytes (default 12000; 0 unlimited).
    #[arg(long, value_name = "N")]
    pub max_bytes: Option<usize>,
}

#[allow(clippy::needless_pass_by_value)]
pub fn run(a: Args) -> Result<u8> {
    let query = PeekQuery::new(
        a.lines.as_deref(),
        a.match_line.as_deref(),
        a.all,
        a.context,
        a.max_lines,
        a.max_bytes,
    )?;
    let bytes = content::load_path(&a.file)?;
    match query.render(&bytes) {
        Ok(result) => {
            std::io::stdout().lock().write_all(&result.bytes)?;
            if let Some(notice) = result.notice() {
                eprintln!("{notice}");
            }
            Ok(exit::SUCCESS)
        }
        Err(PeekError::Locate(error)) => {
            eprintln!("xnip: location not found: {error}");
            Ok(exit::USAGE)
        }
        Err(PeekError::Io(error)) => {
            eprintln!("xnip: io error: {error}");
            Ok(exit::IO)
        }
    }
}
