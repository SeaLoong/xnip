//! Numbered reads with borrowed line cursors and online context merging.

use std::io::Write;

use regex::Regex;
use thiserror::Error;

use crate::core::location::LocateError;

/// Exactly one selection is used by each request.
#[derive(Debug)]
pub enum PeekRange {
    Lines { start: usize, end: usize },
    MatchLine { regex: Regex, context: usize },
    All,
}

#[derive(Debug)]
pub struct PeekOpts {
    pub range: PeekRange,
    /// Maximum selected lines; None is unlimited.
    pub max_lines: Option<usize>,
}

#[derive(Debug, Error)]
pub enum PeekError {
    #[error(transparent)]
    Locate(#[from] LocateError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct PeekResult {
    pub lines_written: usize,
    pub truncated: bool,
}

fn lines(content: &[u8]) -> impl Iterator<Item = &[u8]> {
    content
        .split_inclusive(|byte| *byte == b'\n')
        .map(|line| line.strip_suffix(b"\n").unwrap_or(line))
}

/// Write numbered selected lines without indexing the entire file.
///
/// A search cursor discovers context ranges and a second cursor emits each line
/// at most once. Future context can be read directly from the borrowed bytes;
/// neither the full line index nor a list of matches needs to be retained.
pub fn run<W: Write>(content: &[u8], opts: &PeekOpts, mut out: W) -> Result<PeekResult, PeekError> {
    let cap = opts.max_lines.unwrap_or(usize::MAX);
    let mut cursor = lines(content).enumerate().peekable();
    let mut written = 0usize;
    let mut emit = |start: usize, end: usize| -> Result<bool, PeekError> {
        while let Some((index, _)) = cursor.peek() {
            let number = *index + 1;
            if number > end {
                break;
            }
            let (_, line) = cursor.next().expect("peeked line exists");
            if number < start {
                continue;
            }
            if written == cap {
                return Ok(true);
            }
            write!(out, "{number:>6}: ")?;
            out.write_all(line)?;
            out.write_all(b"\n")?;
            written += 1;
        }
        Ok(false)
    };

    let truncated = match &opts.range {
        PeekRange::Lines { start, end } => {
            if *start == 0 || *end == 0 {
                return Err(LocateError::ZeroLine.into());
            }
            if end < start {
                return Err(LocateError::InvalidRange {
                    start: *start,
                    end: *end,
                }
                .into());
            }
            // Validate before writing, but stop as soon as the requested end is
            // known to exist. An out-of-bounds range still reports the exact total.
            let available = lines(content).take(*end).count();
            if available < *end {
                return Err(LocateError::OutOfBounds {
                    requested: (*start, *end),
                    actual: available,
                }
                .into());
            }
            emit(*start, *end)?
        }
        PeekRange::All => emit(1, usize::MAX)?,
        PeekRange::MatchLine { regex, context } => {
            let mut found = false;
            let mut truncated = false;
            for (index, line) in lines(content).enumerate() {
                if regex.is_match(&String::from_utf8_lossy(line)) {
                    found = true;
                    let number = index + 1;
                    let start = number.saturating_sub(*context).max(1);
                    let end = number.saturating_add(*context);
                    if emit(start, end)? {
                        truncated = true;
                        break;
                    }
                }
            }
            if !found {
                return Err(LocateError::MatchLineNotFound { occurrence: 1 }.into());
            }
            truncated
        }
    };

    Ok(PeekResult {
        lines_written: written,
        truncated,
    })
}
