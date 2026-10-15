//! Text-only unified diff and apply_patch-style import. No fuzzy whitespace matching.
use super::{FileOp, Op, OpContent, Target};
use anyhow::{Context, Result, bail};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone)]
pub struct FilePatch {
    pub hunks: Vec<Hunk>,
    pub create: bool,
    pub delete: bool,
    pub rename_to: Option<PathBuf>,
}
#[derive(Debug, Clone)]
pub struct Hunk {
    position: Option<usize>,
    section: Option<String>,
    eof: bool,
    lines: Vec<Line>,
}
#[derive(Debug, Clone)]
struct Line {
    kind: u8,
    text: String,
    no_newline: bool,
}

/// Git quotes UTF-8 path bytes with C escapes (including octal), not JSON.
fn quoted_path(value: &str) -> Result<String> {
    let bytes = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .context("unterminated quoted patch path")?
        .as_bytes();
    let mut output = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        index += 1;
        if byte != b'\\' {
            output.push(byte);
            continue;
        }
        let escaped = *bytes.get(index).context("truncated path escape")?;
        index += 1;
        let decoded = match escaped {
            b'"' => b'"',
            b'\\' => b'\\',
            b't' => b'\t',
            b'n' => b'\n',
            b'r' => b'\r',
            b'a' => 7,
            b'b' => 8,
            b'f' => 12,
            b'v' => 11,
            b'0'..=b'7' => {
                let mut value = u16::from(escaped - b'0');
                for _ in 0..2 {
                    if let Some(byte @ b'0'..=b'7') = bytes.get(index) {
                        value = value * 8 + u16::from(*byte - b'0');
                        index += 1;
                    } else {
                        break;
                    }
                }
                u8::try_from(value).context("octal path escape exceeds one byte")?
            }
            _ => bail!("unsupported path escape"),
        };
        output.push(decoded);
    }
    String::from_utf8(output).context("patch paths must decode as UTF-8")
}

fn header_prefix(value: &str, prefix: &str) -> Result<bool> {
    let value = value.split('\t').next().unwrap_or(value);
    Ok(if value.starts_with('"') {
        quoted_path(value)?.starts_with(prefix)
    } else {
        value.starts_with(prefix)
    })
}

fn path(value: &str, prefix: Option<&str>, root: &Path) -> Result<PathBuf> {
    let value = value.split('\t').next().unwrap_or(value);
    let decoded;
    let value = if value.starts_with('"') {
        decoded = quoted_path(value)?;
        decoded.as_str()
    } else {
        value
    };
    let value = prefix
        .and_then(|prefix| value.strip_prefix(prefix))
        .unwrap_or(value);
    let result = PathBuf::from(value);
    if value.is_empty()
        || value.ends_with('/')
        || value.contains(['\\', ':', '\0'])
        || result.is_absolute()
        || result
            .components()
            .any(|part| !matches!(part, Component::Normal(_) | Component::CurDir))
    {
        bail!("patch paths must be relative forward-slash paths without parent traversal: {value}");
    }
    let result = root.join(result);
    if !crate::core::transaction::normalize(&result)?.starts_with(root) {
        bail!("patch path escapes working directory: {value}");
    }
    Ok(result)
}

pub fn parse(source: &str) -> Result<Vec<Op>> {
    parse_in(source, &std::env::current_dir()?)
}

/// Resolve patch paths inside an explicit directory without changing process cwd.
pub fn parse_in(source: &str, root: &Path) -> Result<Vec<Op>> {
    let root = std::fs::canonicalize(root)?;
    if !root.is_dir() {
        bail!("patch root must be a directory");
    }
    let source = source
        .strip_prefix('\u{feff}')
        .unwrap_or(source)
        .trim_start_matches(['\r', '\n']);
    let lines: Vec<&str> = source.lines().collect();
    if lines.first() == Some(&"*** Begin Patch") {
        parse_native(&lines, &root)
    } else {
        parse_unified(&lines, &root)
    }
}

fn file(target: PathBuf, operation: FileOp) -> Op {
    Op::File {
        target: Target::File(target),
        operation,
    }
}
fn body_line(raw: &str) -> Result<Line> {
    let kind = *raw
        .as_bytes()
        .first()
        .context("patch body line needs a space, + or - prefix")?;
    if !matches!(kind, b' ' | b'+' | b'-') {
        bail!("invalid patch body prefix");
    }
    Ok(Line {
        kind,
        text: raw[1..].to_string(),
        no_newline: false,
    })
}

fn parse_native(lines: &[&str], root: &Path) -> Result<Vec<Op>> {
    let mut ops = Vec::new();
    let mut index = 1;
    while index < lines.len() {
        let header = lines[index];
        index += 1;
        if header == "*** End Patch" {
            if lines[index..].iter().any(|line| !line.is_empty()) {
                bail!("unexpected text after End Patch");
            }
            return Ok(ops);
        }
        if let Some(value) = header.strip_prefix("*** Add File: ") {
            let target = path(value, None, root)?;
            let mut bytes = Vec::new();
            while index < lines.len() && !lines[index].starts_with("*** ") {
                let content = lines[index]
                    .strip_prefix('+')
                    .context("Add File lines must start with +")?;
                bytes.extend_from_slice(content.as_bytes());
                bytes.push(b'\n');
                index += 1;
            }
            ops.push(file(target, FileOp::Create(OpContent::Text(bytes))));
        } else if let Some(value) = header.strip_prefix("*** Delete File: ") {
            ops.push(file(path(value, None, root)?, FileOp::Delete { was: None }));
        } else if let Some(value) = header.strip_prefix("*** Update File: ") {
            let target = path(value, None, root)?;
            let mut patch = FilePatch {
                hunks: Vec::new(),
                create: false,
                delete: false,
                rename_to: None,
            };
            if let Some(value) = lines
                .get(index)
                .and_then(|line| line.strip_prefix("*** Move to: "))
            {
                patch.rename_to = Some(path(value, None, root)?);
                index += 1;
            }
            while index < lines.len()
                && (lines[index].starts_with("@@") || !lines[index].starts_with("*** "))
            {
                let section = if lines[index] == "@@" {
                    index += 1;
                    None
                } else if let Some(section) = lines[index].strip_prefix("@@ ") {
                    index += 1;
                    Some(section.to_string())
                } else {
                    None
                };
                let mut hunk = Hunk {
                    position: None,
                    section,
                    eof: false,
                    lines: Vec::new(),
                };
                while index < lines.len()
                    && !lines[index].starts_with("@@")
                    && !lines[index].starts_with("*** ")
                {
                    let raw = lines[index];
                    index += 1;
                    if raw == r"\ No newline at end of file" {
                        hunk.lines
                            .last_mut()
                            .context("newline marker without a patch line")?
                            .no_newline = true;
                    } else {
                        hunk.lines
                            .push(body_line(if raw.is_empty() { " " } else { raw })?);
                    }
                }
                if lines.get(index) == Some(&"*** End of File") {
                    hunk.eof = true;
                    index += 1;
                }
                if hunk.lines.is_empty() {
                    bail!("empty update hunk");
                }
                patch.hunks.push(hunk);
            }
            if patch.hunks.is_empty() && patch.rename_to.is_none() {
                bail!("Update File needs hunks or Move to");
            }
            ops.push(file(target, FileOp::Patch(patch)));
        } else {
            bail!("unexpected patch header: {header}");
        }
    }
    bail!("missing End Patch")
}

fn parse_unified(lines: &[&str], root: &Path) -> Result<Vec<Op>> {
    let header = regex::Regex::new(r"^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@.*$")?;
    let mut index = 0;
    let mut ops = Vec::new();
    let mut rename_from = None;
    let mut rename_to = None;
    while index < lines.len() {
        let line = lines[index];
        if line.starts_with("diff --git ") || line.is_empty() {
            if line.starts_with("diff --git ") {
                flush_rename(&mut ops, &mut rename_from, &mut rename_to)?;
            }
            index += 1;
            continue;
        }
        if let Some(value) = line.strip_prefix("rename from ") {
            rename_from = Some(path(value, None, root)?);
            index += 1;
            continue;
        }
        if let Some(value) = line.strip_prefix("rename to ") {
            rename_to = Some(path(value, None, root)?);
            index += 1;
            continue;
        }
        if line.starts_with("index ")
            || line.starts_with("similarity index ")
            || line == "new file mode 100644"
            || line.starts_with("deleted file mode 100")
        {
            index += 1;
            continue;
        }
        let old = line
            .strip_prefix("--- ")
            .context("expected unified --- header; binary/mode-only patches are unsupported")?;
        index += 1;
        let new = lines
            .get(index)
            .and_then(|line| line.strip_prefix("+++ "))
            .context("missing unified +++ header")?;
        index += 1;
        let create = old == "/dev/null";
        let delete = new == "/dev/null";
        if create && delete {
            bail!("both patch paths are /dev/null");
        }
        let git_prefixes =
            (create || header_prefix(old, "a/")?) && (delete || header_prefix(new, "b/")?);
        let old_prefix = git_prefixes.then_some("a/");
        let new_prefix = git_prefixes.then_some("b/");
        let from = if create {
            path(new, new_prefix, root)?
        } else {
            path(old, old_prefix, root)?
        };
        let destination = if delete {
            None
        } else {
            Some(path(new, new_prefix, root)?)
        };
        if let Some(expected) = rename_from.take()
            && expected != from
        {
            bail!("rename/header source mismatch");
        }
        if let Some(expected) = rename_to.take()
            && Some(expected) != destination
        {
            bail!("rename/header destination mismatch");
        }
        let rename_to = destination.filter(|to| *to != from);
        let mut patch = FilePatch {
            hunks: Vec::new(),
            create,
            delete,
            rename_to,
        };
        while index < lines.len() && lines[index].starts_with("@@") {
            let captures = header
                .captures(lines[index])
                .context("invalid unified hunk header")?;
            index += 1;
            let start: usize = captures[1].parse()?;
            let old_count: usize = captures.get(2).map_or("1", |m| m.as_str()).parse()?;
            let new_start: usize = captures[3].parse()?;
            let new_count: usize = captures.get(4).map_or("1", |m| m.as_str()).parse()?;
            if (old_count > 0 && start == 0) || (new_count > 0 && new_start == 0) {
                bail!("nonempty hunk has zero line number");
            }
            let mut hunk = Hunk {
                position: Some(if old_count == 0 { start } else { start - 1 }),
                section: None,
                eof: false,
                lines: Vec::new(),
            };
            let (mut removed, mut added) = (0, 0);
            while removed < old_count || added < new_count {
                let raw = lines.get(index).context("truncated unified hunk")?;
                index += 1;
                if *raw == r"\ No newline at end of file" {
                    hunk.lines
                        .last_mut()
                        .context("newline marker without a patch line")?
                        .no_newline = true;
                    continue;
                }
                let item = body_line(raw)?;
                if item.kind != b'+' {
                    removed += 1;
                }
                if item.kind != b'-' {
                    added += 1;
                }
                if removed > old_count || added > new_count {
                    bail!("hunk line counts do not match header");
                }
                hunk.lines.push(item);
            }
            if lines.get(index) == Some(&r"\ No newline at end of file") {
                hunk.lines
                    .last_mut()
                    .context("newline marker without a patch line")?
                    .no_newline = true;
                index += 1;
            }
            patch.hunks.push(hunk);
        }
        if patch.hunks.is_empty() && !create && !delete && patch.rename_to.is_none() {
            bail!("no hunks in update");
        }
        ops.push(file(from, FileOp::Patch(patch)));
    }
    flush_rename(&mut ops, &mut rename_from, &mut rename_to)?;
    if ops.is_empty() {
        bail!("no file operations in patch");
    }
    Ok(ops)
}
fn flush_rename(
    ops: &mut Vec<Op>,
    from: &mut Option<PathBuf>,
    to: &mut Option<PathBuf>,
) -> Result<()> {
    match (from.take(), to.take()) {
        (Some(from), Some(to)) => ops.push(file(from, FileOp::Rename { to, was: None })),
        (None, None) => {}
        _ => bail!("incomplete rename metadata"),
    }
    Ok(())
}

#[derive(Clone)]
struct TextLine {
    text: String,
    ending: String,
    eof_required: bool,
}
fn text_lines(bytes: &[u8]) -> Result<Vec<TextLine>> {
    let text = std::str::from_utf8(bytes).context("text patches require UTF-8 files")?;
    Ok(text
        .split_inclusive('\n')
        .map(|line| {
            let (text, ending) = if let Some(text) = line.strip_suffix("\r\n") {
                (text, "\r\n")
            } else if let Some(text) = line.strip_suffix('\n') {
                (text, "\n")
            } else {
                (line, "")
            };
            TextLine {
                text: text.into(),
                ending: ending.into(),
                eof_required: false,
            }
        })
        .collect())
}

pub fn apply(before: &[u8], patch: &FilePatch) -> Result<Vec<u8>> {
    let original = text_lines(before)?;
    let newline = original
        .iter()
        .find(|line| !line.ending.is_empty())
        .map_or("\n", |line| line.ending.as_str());
    let mut output: Vec<TextLine> = Vec::new();
    let mut cursor = 0;
    for hunk in &patch.hunks {
        let old: Vec<&Line> = hunk.lines.iter().filter(|line| line.kind != b'+').collect();
        let mut lower = cursor;
        if let Some(section) = &hunk.section {
            let found: Vec<_> = original
                .iter()
                .enumerate()
                .skip(cursor)
                .filter(|(_, line)| line.text == *section)
                .map(|(index, _)| index)
                .collect();
            if found.len() != 1 {
                bail!("patch section is missing or ambiguous: {section}");
            }
            lower = found[0] + 1;
        }
        let matches = |position: usize| {
            position >= lower
                && position <= original.len()
                && old.len() <= original.len() - position
                && (!hunk.eof || position + old.len() == original.len())
                && old
                    .iter()
                    .zip(&original[position..])
                    .all(|(expected, actual)| {
                        expected.text == actual.text
                            && (!expected.no_newline || actual.ending.is_empty())
                    })
        };
        let position = if let Some(position) = hunk.position.filter(|&position| matches(position)) {
            position
        } else if old.is_empty() {
            if hunk.position.is_none() && (hunk.eof || original.is_empty()) {
                original.len()
            } else {
                bail!("insertion hunk requires context or an explicit valid position");
            }
        } else {
            let candidates: Vec<_> = (lower..=original.len())
                .filter(|&position| matches(position))
                .take(2)
                .collect();
            if candidates.len() != 1 {
                bail!("patch context is missing or ambiguous; provide more context");
            }
            candidates[0]
        };
        output.extend_from_slice(&original[cursor..position]);
        let mut current = position;
        for line in &hunk.lines {
            match line.kind {
                b' ' => {
                    let mut context = original[current].clone();
                    context.eof_required = line.no_newline;
                    output.push(context);
                    current += 1;
                }
                b'-' => {
                    current += 1;
                }
                b'+' => output.push(TextLine {
                    text: line.text.clone(),
                    eof_required: line.no_newline,
                    ending: if line.no_newline {
                        String::new()
                    } else {
                        newline.into()
                    },
                }),
                _ => unreachable!(),
            }
        }
        cursor = current;
    }
    output.extend_from_slice(&original[cursor..]);
    let mut bytes = Vec::new();
    for (index, line) in output.iter().enumerate() {
        if line.eof_required && index + 1 < output.len() {
            bail!("no-newline marker is not at the end of the file");
        }
        bytes.extend_from_slice(line.text.as_bytes());
        bytes.extend_from_slice(if line.ending.is_empty() && index + 1 < output.len() {
            newline.as_bytes()
        } else {
            line.ending.as_bytes()
        });
    }
    if patch.delete && !bytes.is_empty() {
        bail!("delete patch did not remove the entire file");
    }
    Ok(bytes)
}
