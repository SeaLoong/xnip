//! `insert`：在指定行 before/after 插入内容。
//!
//! 设计要点：
//! - 单点插入（不支持区间 Locator；区间用 `replace`）
//! - 插入内容支持任意字节
//! - 插入内容若不以 `\n` 结尾，自动补一个（保证插入后仍然是整行结构）
//!
//! Revert（PLAN §6.8）：`insert/replace` 互逆；insert 的反向通过 `replace` 删除
//! 对应行段实现。本模块只关心 forward 计算。

use thiserror::Error;

use crate::core::location::split_lines;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Position {
    Before,
    After,
}

#[derive(Debug, Error)]
pub enum InsertError {
    #[error("anchor line {line} is out of bounds (file has {total} lines)")]
    OutOfBounds { line: usize, total: usize },
    #[error("zero is not a valid 1-based line number")]
    ZeroLine,
}

/// 在 `anchor_line`（1-based）的 `position` 处插入 `payload`，返回新内容字节流。
///
/// - 空文件 + `before line=1` → 插入到最前
/// - `after total_lines` → 插入到末尾
/// - `payload` 末尾不以 `\n` 结尾时自动补 `\n`
///
/// # Errors
/// 见 [`InsertError`]。
pub fn insert_at(
    content: &[u8],
    anchor_line: usize,
    position: Position,
    payload: &[u8],
) -> Result<Vec<u8>, InsertError> {
    if anchor_line == 0 {
        return Err(InsertError::ZeroLine);
    }

    let lines = split_lines(content);
    let total = lines.len();

    // 空文件特殊处理：only line=1 + Before/After 都视为"写到文件开头"
    if total == 0 {
        if anchor_line != 1 {
            return Err(InsertError::OutOfBounds {
                line: anchor_line,
                total,
            });
        }
        return Ok(normalize(payload));
    }

    if anchor_line > total {
        return Err(InsertError::OutOfBounds {
            line: anchor_line,
            total,
        });
    }

    // 重建：把 lines 重新 join（带 \n），在指定位置插入 payload
    let insert_idx = match position {
        Position::Before => anchor_line - 1, // 0-based
        Position::After => anchor_line,      // 0-based 之后
    };

    let mut out = Vec::with_capacity(content.len() + payload.len() + 1);
    for (i, line) in lines.iter().enumerate() {
        if i == insert_idx {
            out.extend_from_slice(&normalize(payload));
        }
        out.extend_from_slice(line);
        // 还原行尾 \n：只要原文件该行后面有 \n 就补一个
        if has_newline_after(content, line) {
            out.push(b'\n');
        }
    }
    if insert_idx == lines.len() {
        out.extend_from_slice(&normalize(payload));
    }

    Ok(out)
}

/// payload 不以 `\n` 结尾时补一个，保证插入后仍然是行结构。
fn normalize(payload: &[u8]) -> Vec<u8> {
    if payload.is_empty() {
        return Vec::new();
    }
    if payload.last() == Some(&b'\n') {
        payload.to_vec()
    } else {
        let mut v = Vec::with_capacity(payload.len() + 1);
        v.extend_from_slice(payload);
        v.push(b'\n');
        v
    }
}

/// 判断 line 在原 content 中是否后跟 `\n`（用于决定输出该行后是否补 \n）。
///
/// 注意：`split_lines` 给出的 line 不含 `\n`，但其在 content 中的字节范围紧邻一个 `\n`
/// （除非是最后一行且文件不以 `\n` 结尾）。我们用指针差比较位置。
fn has_newline_after(content: &[u8], line: &[u8]) -> bool {
    // line 是 content 的 sub-slice
    let base = content.as_ptr() as usize;
    let line_start = line.as_ptr() as usize;
    let line_end_idx = line_start - base + line.len();
    content.get(line_end_idx) == Some(&b'\n')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_before_first_line() {
        let r = insert_at(b"a\nb\n", 1, Position::Before, b"X").unwrap();
        assert_eq!(r, b"X\na\nb\n");
    }

    #[test]
    fn insert_after_last_line() {
        let r = insert_at(b"a\nb\n", 2, Position::After, b"X").unwrap();
        assert_eq!(r, b"a\nb\nX\n");
    }

    #[test]
    fn insert_after_middle() {
        let r = insert_at(b"a\nb\nc\n", 2, Position::After, b"X").unwrap();
        assert_eq!(r, b"a\nb\nX\nc\n");
    }

    #[test]
    fn insert_multiline_payload() {
        let r = insert_at(b"a\nb\n", 1, Position::After, b"X\nY").unwrap();
        assert_eq!(r, b"a\nX\nY\nb\n");
    }

    #[test]
    fn insert_into_empty_file() {
        let r = insert_at(b"", 1, Position::Before, b"X").unwrap();
        assert_eq!(r, b"X\n");
    }

    #[test]
    fn insert_into_file_without_trailing_newline() {
        let r = insert_at(b"a\nb", 2, Position::After, b"X").unwrap();
        // 原文件最后一行 'b' 后面没有 \n，输出也保持
        assert_eq!(r, b"a\nbX\n");
    }

    #[test]
    fn insert_zero_line_errors() {
        let e = insert_at(b"a\n", 0, Position::Before, b"X").unwrap_err();
        assert!(matches!(e, InsertError::ZeroLine));
    }

    #[test]
    fn insert_out_of_bounds_errors() {
        let e = insert_at(b"a\n", 5, Position::After, b"X").unwrap_err();
        assert!(matches!(e, InsertError::OutOfBounds { .. }));
    }

    #[test]
    fn payload_with_trailing_newline_not_double_appended() {
        let r = insert_at(b"a\n", 1, Position::Before, b"X\n").unwrap();
        assert_eq!(r, b"X\na\n");
    }

    #[test]
    fn empty_payload_inserts_nothing() {
        let r = insert_at(b"a\nb\n", 1, Position::After, b"").unwrap();
        assert_eq!(r, b"a\nb\n");
    }
}
