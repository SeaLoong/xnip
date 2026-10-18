//! CLI adapter for bounded multi-file search.

use std::io::Write;
use std::path::PathBuf;

use anyhow::Result;
use clap::Args as ClapArgs;

use crate::core::read::FindQuery;
use crate::output::exit;

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Files to search, in output order.
    #[arg(required = true, num_args = 1..)]
    pub files: Vec<PathBuf>,
    /// Byte regex; emits path:line:column.
    #[arg(long, value_name = "REGEX", conflicts_with = "match_line")]
    pub pattern: Option<String>,
    /// Line regex; emits path:line.
    #[arg(long, value_name = "REGEX")]
    pub match_line: Option<String>,
    /// Maximum total hits (default 100; 0 unlimited).
    #[arg(long, value_name = "N")]
    pub max_matches: Option<usize>,
    /// Maximum output bytes (default 12000; 0 unlimited).
    #[arg(long, value_name = "N")]
    pub max_bytes: Option<usize>,
    /// Keep only the first hit per file.
    #[arg(long)]
    pub first_only: bool,
}

#[allow(clippy::needless_pass_by_value)]
pub fn run(a: Args) -> Result<u8> {
    let result = FindQuery::new(
        a.pattern.as_deref(),
        a.match_line.as_deref(),
        a.first_only,
        a.max_matches,
        a.max_bytes,
    )?
    .run(&a.files)?;
    std::io::stdout().lock().write_all(&result.bytes)?;
    if let Some(notice) = result.notice() {
        eprintln!("{notice}");
    }
    for error in &result.load_errors {
        eprintln!("xnip find: {error}");
    }
    Ok(if result.load_error_count > 0 {
        exit::IO
    } else if result.items == 0 {
        exit::USAGE
    } else {
        exit::SUCCESS
    })
}
