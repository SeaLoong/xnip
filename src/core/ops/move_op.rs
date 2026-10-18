//! `move`：把行块从 `[from_start, from_end]` 移动到目标行 `to_line` 的 before/after。
//!
//! 文件名 `move_op.rs` 因 `move` 是 Rust 关键字。
//!
//! 算法：
//! 1. 抽取源区段字节（含其各行尾 `\n`，但最后一行 \n 仅当原本就有）
//! 2. 计算"删源后"目标行号的位移：
//!    - 若 `to_line < from_start` → 不变
//!    - 若 `to_line > from_end` → `to_line -= (from_end - from_start + 1)`
//!    - 否则（区间内或紧邻）→ 视为退化为 noop 或报错
//! 3. 在删源后的内容上做 insert
//!
//! Revert：`move from S..E to T pos` 的反向 = `move from T'..T'+(E-S) to S' pos'`
//! 计算复杂；本模块仅实现 forward，revert 由 cli 层根据已知参数构造再次调用 forward。

use thiserror::Error;

use crate::core::location::split_lines;
use crate::core::ops::insert::{Position, insert_at};
use crate::core::ops::replace::replace_range;

#[derive(Debug, Error)]
pub enum MoveError {
    #[error("source range {from_start}-{from_end} is invalid for file with {total} lines")]
    InvalidSource {
        from_start: usize,
        from_end: usize,
        total: usize,
    },
    #[error("target line {to} is inside the source range {from_start}-{from_end}")]
    TargetInsideSource {
        from_start: usize,
        from_end: usize,
        to: usize,
    },
    #[error("zero is not a valid 1-based line number")]
    ZeroLine,
    #[error("internal error during move: {0}")]
    Internal(String),
}

/// 把 `[from_start, from_end]` 行块移动到 `to_line` 的 `position`（before/after）。
///
/// # Errors
/// 见 [`MoveError`]。
pub fn move_lines(
    content: &[u8],
    from_start: usize,
    from_end: usize,
    to_line: usize,
    position: Position,
) -> Result<Vec<u8>, MoveError> {
    if from_start == 0 || from_end == 0 || to_line == 0 {
        return Err(MoveError::ZeroLine);
    }
    if from_end < from_start {
        return Err(MoveError::InvalidSource {
            from_start,
            from_end,
            total: 0,
        });
    }
    let lines = split_lines(content);
    let total = lines.len();
    if from_end > total {
        return Err(MoveError::InvalidSource {
            from_start,
            from_end,
            total,
        });
    }
    if to_line > total {
        return Err(MoveError::InvalidSource {
            from_start: to_line,
            from_end: to_line,
            total,
        });
    }

    // 目标在源内 → 退化为 noop（原地不动）
    if to_line >= from_start && to_line <= from_end {
        return Err(MoveError::TargetInsideSource {
            from_start,
            from_end,
            to: to_line,
        });
    }

    // Step 1: 提取源区段字节
    let payload = extract_lines_with_newline(content, from_start, from_end);

    // Step 2: 删源
    let after_remove = replace_range(content, from_start, from_end, b"")
        .map_err(|e| MoveError::Internal(format!("{e}")))?;

    // Step 3: 计算调整后的目标行号
    let span = from_end - from_start + 1;
    let adjusted_to = if to_line < from_start {
        to_line
    } else {
        // to_line > from_end
        to_line - span
    };

    // Step 4: 插入
    let mut result = insert_at(&after_remove, adjusted_to, position, &payload)
        .map_err(|e| MoveError::Internal(format!("{e}")))?;
    if !content.ends_with(b"\n") && result.ends_with(b"\n") {
        result.pop();
    }
    Ok(result)
}

/// 计算 forward `move from [S, E] to T pos` 的反向参数。
///
/// 给定 forward 后的内容（行号语义已变化），调用 `move_lines` 用以下参数即可还原：
///
/// `(S', E', T', pos')` 满足：
/// - `[S', E']` 是 forward 之后块所在的新行号区间
/// - `(T', pos')` 表达"原 S 紧邻位置"在 forward 后内容的行号定位
///
/// # Errors / 不可逆条件
/// - `T` 在 `[S, E]` 区间内（forward 本身已被禁止）
/// - `T == S - 1` 且 `pos == After`，或 `T == E + 1` 且 `pos == Before`：noop（无位移）
///
/// # Panics
/// 不可能发生的算术 underflow（已在前置 if 中过滤）。
pub fn reverse_params(
    from_start: usize,
    from_end: usize,
    to_line: usize,
    pos: Position,
) -> Result<(usize, usize, usize, Position), MoveError> {
    if from_start == 0 || from_end == 0 || to_line == 0 {
        return Err(MoveError::ZeroLine);
    }
    if from_end < from_start {
        return Err(MoveError::InvalidSource {
            from_start,
            from_end,
            total: 0,
        });
    }
    if to_line >= from_start && to_line <= from_end {
        return Err(MoveError::TargetInsideSource {
            from_start,
            from_end,
            to: to_line,
        });
    }
    let len = from_end - from_start + 1;
    if (to_line == from_start - 1 && pos == Position::After)
        || (to_line.checked_sub(1) == Some(from_end) && pos == Position::Before)
    {
        return Ok((from_start, from_end, to_line, pos));
    }

    let (block_lo, block_hi);
    let (target, target_pos);
    if to_line < from_start {
        // T < S
        match pos {
            Position::Before => {
                block_lo = to_line;
                block_hi = to_line + len - 1;
            }
            Position::After => {
                block_lo = to_line + 1;
                block_hi = to_line + len;
            }
        }
        // 原 S-1 行 forward 后位置 = (S-1)+L; 把块塞到它之后
        target = (from_start - 1) + len;
        target_pos = Position::After;
    } else {
        // T > E
        match pos {
            Position::Before => {
                block_lo = to_line - len;
                block_hi = to_line - 1;
            }
            Position::After => {
                block_lo = to_line - len + 1;
                block_hi = to_line;
            }
        }
        // 原 E+1 行 forward 后位置 = (E+1) - L; 块塞到它之前
        target = (from_end + 1) - len;
        target_pos = Position::Before;
    }
    Ok((block_lo, block_hi, target, target_pos))
}

/// 提取 `[start, end]` 行（含其行尾 `\n`，仅当原本就有）的字节序列。
fn extract_lines_with_newline(content: &[u8], start: usize, end: usize) -> Vec<u8> {
    let lines = split_lines(content);
    let mut out = Vec::new();
    let base = content.as_ptr() as usize;
    for line in lines.iter().take(end).skip(start - 1) {
        out.extend_from_slice(line);
        let line_start = line.as_ptr() as usize;
        let end_idx = line_start - base + line.len();
        if content.get(end_idx) == Some(&b'\n') {
            out.push(b'\n');
        }
    }
    out
}
