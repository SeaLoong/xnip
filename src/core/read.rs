//! Shared read requests: validation, limits, scanning and bounded rendering.
//! CLI and MCP are adapters over this seam; output policy is defined once here.

use std::io::{self, Write};
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use regex::{Regex, bytes::Regex as ByteRegex};

use crate::core::content;
use crate::core::ops::find::{FindMode, FindOpts, visit};
use crate::core::ops::peek::{PeekError, PeekOpts, PeekRange, run as run_peek};

pub const DEFAULT_MAX_LINES: usize = 200;
pub const DEFAULT_MAX_MATCHES: usize = 100;
pub use crate::output::budget::DEFAULT_MAX_BYTES;

fn limit(value: Option<usize>, default: usize) -> Option<usize> {
    match value.unwrap_or(default) {
        0 => None,
        value => Some(value),
    }
}

/// Retains only the budget plus four lookahead bytes for safe UTF-8 cuts.
struct BoundedText {
    bytes: Vec<u8>,
    max_bytes: usize,
    total_bytes: usize,
}

impl BoundedText {
    fn new(max_bytes: usize) -> Self {
        Self {
            bytes: Vec::new(),
            max_bytes,
            total_bytes: 0,
        }
    }

    fn saturated(&self) -> bool {
        self.max_bytes > 0 && self.bytes.len() >= self.max_bytes.saturating_add(4)
    }

    fn finish(mut self) -> (Vec<u8>, bool) {
        let retained = crate::output::budget::prefix(&self.bytes, self.max_bytes).len();
        self.bytes.truncate(retained);
        let truncated = retained < self.total_bytes;
        (self.bytes, truncated)
    }
}

impl Write for BoundedText {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.total_bytes = self.total_bytes.saturating_add(bytes.len());
        let remaining = if self.max_bytes == 0 {
            bytes.len()
        } else {
            self.max_bytes
                .saturating_add(4)
                .saturating_sub(self.bytes.len())
        };
        self.bytes
            .extend_from_slice(&bytes[..bytes.len().min(remaining)]);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct ReadOutput {
    pub bytes: Vec<u8>,
    pub items: usize,
    pub truncated: bool,
    pub load_errors: Vec<String>,
    pub load_error_count: usize,
    max_bytes: usize,
    item_limit: &'static str,
}

impl ReadOutput {
    pub fn text(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(&self.bytes)
    }

    pub fn notice(&self) -> Option<String> {
        self.truncated.then(|| {
            format!(
                "[truncated: selected {} items; set {}=0 and max_bytes=0 for full output]",
                self.items, self.item_limit
            )
        })
    }

    /// Partial failure text shares the payload budget; notices add fixed overhead.
    pub fn into_text(self) -> String {
        let notice = self.notice();
        let mut text = String::from_utf8_lossy(&self.bytes).into_owned();
        if self.load_error_count > 0 {
            text.push('\n');
            text.push_str(&self.load_errors.join("\n"));
        }
        let mut text = crate::output::budget::truncate(&text, self.max_bytes);
        // Failure counts must survive even when the match payload filled the budget.
        if self.load_error_count > 0 {
            use std::fmt::Write as _;
            let _ = write!(
                text,
                "\n[{} load error(s); {} details retained]",
                self.load_error_count,
                self.load_errors.len()
            );
        }
        if let Some(notice) = notice {
            if !text.ends_with('\n') {
                text.push('\n');
            }
            text.push_str(&notice);
        }
        text
    }
}

#[derive(Debug)]
pub struct PeekQuery {
    opts: PeekOpts,
    max_bytes: usize,
}

impl PeekQuery {
    pub fn new(
        lines: Option<&str>,
        match_line: Option<&str>,
        all: bool,
        context: usize,
        max_lines: Option<usize>,
        max_bytes: Option<usize>,
    ) -> Result<Self> {
        if usize::from(lines.is_some()) + usize::from(match_line.is_some()) + usize::from(all) != 1
        {
            bail!("choose exactly one of lines, match_line, all");
        }
        if context > 0 && match_line.is_none() {
            bail!("context requires match_line");
        }
        let range = if let Some(lines) = lines {
            let parse = |value: &str| value.trim().parse::<usize>().context("invalid line number");
            let (start, end) = if let Some((start, end)) = lines.split_once('-') {
                (parse(start)?, parse(end)?)
            } else {
                let line = parse(lines)?;
                (line, line)
            };
            if start == 0 || end < start {
                bail!("lines must be a positive, ascending range");
            }
            PeekRange::Lines { start, end }
        } else if let Some(pattern) = match_line {
            PeekRange::MatchLine {
                regex: Regex::new(pattern).context("invalid match_line regex")?,
                context,
            }
        } else {
            PeekRange::All
        };
        Ok(Self {
            opts: PeekOpts {
                range,
                max_lines: limit(max_lines, DEFAULT_MAX_LINES),
            },
            max_bytes: max_bytes.unwrap_or(DEFAULT_MAX_BYTES),
        })
    }

    pub fn render(&self, content: &[u8]) -> Result<ReadOutput, PeekError> {
        let mut output = BoundedText::new(self.max_bytes);
        let result = run_peek(content, &self.opts, &mut output)?;
        let (bytes, bytes_truncated) = output.finish();
        Ok(ReadOutput {
            bytes,
            items: result.lines_written,
            truncated: result.truncated || bytes_truncated,
            load_errors: Vec::new(),
            load_error_count: 0,
            max_bytes: self.max_bytes,
            item_limit: "max_lines",
        })
    }
}

#[derive(Debug)]
enum Search {
    Line(Regex),
    Pattern(ByteRegex),
}

#[derive(Debug)]
pub struct FindQuery {
    search: Search,
    max_matches: Option<usize>,
    max_bytes: usize,
    first_only: bool,
}

impl FindQuery {
    pub fn new(
        pattern: Option<&str>,
        match_line: Option<&str>,
        first_only: bool,
        max_matches: Option<usize>,
        max_bytes: Option<usize>,
    ) -> Result<Self> {
        let search = match (pattern, match_line) {
            (Some(pattern), None) => {
                Search::Pattern(ByteRegex::new(pattern).context("invalid pattern regex")?)
            }
            (None, Some(pattern)) => {
                Search::Line(Regex::new(pattern).context("invalid match_line regex")?)
            }
            _ => bail!("choose exactly one of pattern, match_line"),
        };
        Ok(Self {
            search,
            max_matches: limit(max_matches, DEFAULT_MAX_MATCHES),
            max_bytes: max_bytes.unwrap_or(DEFAULT_MAX_BYTES),
            first_only,
        })
    }

    pub fn run(&self, files: &[PathBuf]) -> Result<ReadOutput> {
        if files.is_empty() {
            bail!("files must contain at least one path");
        }
        let mut output = BoundedText::new(self.max_bytes);
        let mut items = 0usize;
        let mut truncated = false;
        let mut load_errors = Vec::new();
        let mut load_error_count = 0usize;
        for path in files {
            let bytes = match content::load_path(path) {
                Ok(bytes) => bytes,
                Err(error) => {
                    load_error_count += 1;
                    if load_errors.len() < 8 {
                        load_errors.push(format!("{}: {error}", path.display()));
                    }
                    continue;
                }
            };
            let remaining = self.max_matches.map(|cap| cap.saturating_sub(items));
            // One extra match proves truncation without retaining every match.
            let opts = FindOpts {
                mode: match &self.search {
                    Search::Line(regex) => FindMode::MatchLine(regex),
                    Search::Pattern(regex) => FindMode::Pattern(regex),
                },
                max_matches: remaining.map(|cap| cap.saturating_add(1)),
                first_only: self.first_only,
            };
            let mut write_error = None;
            visit(&bytes, &opts, |hit| {
                if self.max_matches.is_some_and(|cap| items >= cap) {
                    truncated = true;
                    return false;
                }
                // Once the prefix and UTF-8 lookahead are retained, only count
                // selected hits. This preserves receipts without allocating or
                // formatting discarded matches, including unlimited-hit searches.
                if !output.saturated() {
                    let result = if hit.col > 0 {
                        writeln!(output, "{}:{}:{}", path.display(), hit.line, hit.col)
                    } else {
                        writeln!(output, "{}:{}", path.display(), hit.line)
                    };
                    if let Err(error) = result {
                        write_error = Some(error);
                        return false;
                    }
                }
                items += 1;
                true
            });
            if let Some(error) = write_error {
                return Err(error.into());
            }
        }
        let (bytes, bytes_truncated) = output.finish();
        Ok(ReadOutput {
            bytes,
            items,
            truncated: truncated || bytes_truncated,
            load_errors,
            load_error_count,
            max_bytes: self.max_bytes,
            item_limit: "max_matches",
        })
    }
}
