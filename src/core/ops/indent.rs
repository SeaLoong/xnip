//! `indent`：缩进调整 / tab-space 互转。
//!
//! 4 种算子：
//! - `Add(n)`：每行行首加 `n` 个空格
//! - `Remove(n)`：每行行首删去最多 `n` 个空格（不足则尽量删）
//! - `TabsToSpaces(n)`：行首每个 `\t` 展开为 `n` 个空格
//! - `SpacesToTabs(n)`：行首每 `n` 个连续空格折叠为 `\t`
//!
//! Revert：
//! - `Add(n)` ↔ `Remove(n)`
//! - `TabsToSpaces(n)` ↔ `SpacesToTabs(n)`（前提：区域内行首空格数都是 n 的倍数）
//!
//! 仅对每行 **行首** 起效，不影响行内其它位置。

use thiserror::Error;

use crate::core::location::split_lines;

#[derive(Debug, Clone, Copy)]
pub enum IndentOp {
    Add(usize),
    Remove(usize),
    TabsToSpaces(usize),
    SpacesToTabs(usize),
}

#[derive(Debug, Error)]
pub enum IndentError {
    #[error("line range {start}-{end} is out of bounds (file has {total} lines)")]
    OutOfBounds {
        start: usize,
        end: usize,
        total: usize,
    },
    #[error("zero is not a valid 1-based line number")]
    ZeroLine,
    #[error("invalid range: end ({end}) < start ({start})")]
    InvalidRange { start: usize, end: usize },
    #[error("indent unit must be >= 1")]
    ZeroUnit,
}

/// 在 `[start_line, end_line]` 区间内对每行行首应用 `op`，返回新字节流。
///
/// # Errors
/// 见 [`IndentError`]。
pub fn apply_indent(
    content: &[u8],
    start_line: usize,
    end_line: usize,
    op: IndentOp,
) -> Result<Vec<u8>, IndentError> {
    if start_line == 0 || end_line == 0 {
        return Err(IndentError::ZeroLine);
    }
    if end_line < start_line {
        return Err(IndentError::InvalidRange {
            start: start_line,
            end: end_line,
        });
    }
    let lines = split_lines(content);
    let total = lines.len();
    if end_line > total {
        return Err(IndentError::OutOfBounds {
            start: start_line,
            end: end_line,
            total,
        });
    }

    if let IndentOp::Add(0) | IndentOp::Remove(0) = op {
        // 0 是 noop
    }
    if matches!(op, IndentOp::TabsToSpaces(0) | IndentOp::SpacesToTabs(0)) {
        return Err(IndentError::ZeroUnit);
    }

    let base = content.as_ptr() as usize;
    let mut out = Vec::with_capacity(content.len());
    for (i, line) in lines.iter().enumerate() {
        let n = i + 1;
        let new_line: Vec<u8>;
        let line_bytes: &[u8] = if n >= start_line && n <= end_line {
            new_line = apply_op_to_line(line, op);
            &new_line
        } else {
            line
        };
        out.extend_from_slice(line_bytes);
        // 还原行尾 \n
        let line_start = line.as_ptr() as usize;
        let end_idx = line_start - base + line.len();
        if content.get(end_idx) == Some(&b'\n') {
            out.push(b'\n');
        }
    }

    Ok(out)
}

/// 对单行（不含 `\n`）的行首应用 op，返回新行字节。
fn apply_op_to_line(line: &[u8], op: IndentOp) -> Vec<u8> {
    match op {
        IndentOp::Add(n) => {
            let mut v = Vec::with_capacity(line.len() + n);
            v.extend(std::iter::repeat_n(b' ', n));
            v.extend_from_slice(line);
            v
        }
        IndentOp::Remove(n) => {
            let mut to_remove = n;
            let mut idx = 0;
            while to_remove > 0 && line.get(idx) == Some(&b' ') {
                idx += 1;
                to_remove -= 1;
            }
            line[idx..].to_vec()
        }
        IndentOp::TabsToSpaces(unit) => {
            let mut v = Vec::with_capacity(line.len());
            let mut i = 0;
            while i < line.len() && line[i] == b'\t' {
                v.extend(std::iter::repeat_n(b' ', unit));
                i += 1;
            }
            v.extend_from_slice(&line[i..]);
            v
        }
        IndentOp::SpacesToTabs(unit) => {
            let mut leading_spaces = 0usize;
            while line.get(leading_spaces) == Some(&b' ') {
                leading_spaces += 1;
            }
            let tabs = leading_spaces / unit;
            let leftover = leading_spaces % unit;
            let mut v = Vec::with_capacity(line.len());
            v.extend(std::iter::repeat_n(b'\t', tabs));
            v.extend(std::iter::repeat_n(b' ', leftover));
            v.extend_from_slice(&line[leading_spaces..]);
            v
        }
    }
}
