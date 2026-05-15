//! `replace`：替换/删除（空文本即删除）。
//!
//! 两套核心路径（PLAN §6.7.3）：
//!
//! 1. **range 模式**（`Lines` / `MatchLine` / `Between` / `BetweenRe`）
//!    - 把 `[start_line, end_line]` 整段替换为 payload
//!    - payload 末尾若不以 `\n` 结尾且原区段最后一行有 `\n`，自动补 `\n`
//!    - payload == 空字节 → 等价于删除该区段（连同其行尾 `\n`）
//!
//! 2. **pattern 模式**
//!    - 用 `regex::bytes::Regex::replacen` 在字节流上替换前 N 处或全部
//!    - replacement 字符串支持 regex crate 原生的 `$1` / `${name}` 反向引用
//!
//! `--was` 校验在 cli 层进行（这里只关心纯函数计算）。

use thiserror::Error;

use crate::core::location::{Count, split_lines};

#[derive(Debug, Error)]
pub enum ReplaceError {
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
}

/// 把 `[start_line, end_line]`（1-based 闭区间）替换为 `payload`，返回新字节流。
///
/// payload 为空 → 等价于"删除该区段（含其行尾 \n）"。
///
/// # Errors
/// 见 [`ReplaceError`]。
pub fn replace_range(
    content: &[u8],
    start_line: usize,
    end_line: usize,
    payload: &[u8],
) -> Result<Vec<u8>, ReplaceError> {
    if start_line == 0 || end_line == 0 {
        return Err(ReplaceError::ZeroLine);
    }
    if end_line < start_line {
        return Err(ReplaceError::InvalidRange {
            start: start_line,
            end: end_line,
        });
    }

    let lines = split_lines(content);
    let total = lines.len();
    if end_line > total {
        return Err(ReplaceError::OutOfBounds {
            start: start_line,
            end: end_line,
            total,
        });
    }

    // 还原"该行后是否有 \n"的信息（split_lines 丢了分隔符）
    let line_has_newline = compute_newline_flags(content, &lines);

    let last_in_range_has_nl = line_has_newline[end_line - 1];

    let mut out = Vec::with_capacity(content.len());

    // 写入 [1, start_line)
    for i in 0..(start_line - 1) {
        out.extend_from_slice(lines[i]);
        if line_has_newline[i] {
            out.push(b'\n');
        }
    }

    // 写入 payload
    if !payload.is_empty() {
        out.extend_from_slice(payload);
        // 如果原区段最后一行带 \n，并且 payload 末尾不是 \n，补一个
        if last_in_range_has_nl && payload.last() != Some(&b'\n') {
            out.push(b'\n');
        }
    }

    // 写入 (end_line, total]
    for i in end_line..total {
        out.extend_from_slice(lines[i]);
        if line_has_newline[i] {
            out.push(b'\n');
        }
    }

    Ok(out)
}

/// 用 regex 在字节流上替换前 N 处（或全部），返回 `(new_bytes, n_replaced)`。
///
/// `replacement` 支持 `$1` / `${name}` 反向引用。
pub fn replace_pattern(
    content: &[u8],
    re: &regex::bytes::Regex,
    replacement: &str,
    count: Count,
) -> (Vec<u8>, usize) {
    // regex::bytes::Regex 的 `replacen` limit=0 表示全部
    let limit = match count {
        Count::All => 0,
        Count::First(n) => n,
    };
    // 先数命中以便返回 n_replaced
    let n_replaced = match count {
        Count::All => re.find_iter(content).count(),
        Count::First(n) => re.find_iter(content).take(n).count(),
    };
    let new_bytes = re
        .replacen(content, limit, replacement.as_bytes())
        .into_owned();
    (new_bytes, n_replaced)
}

/// 给定 `split_lines` 结果，反推每行后面在原 content 中是否跟着 `\n`。
fn compute_newline_flags(content: &[u8], lines: &[&[u8]]) -> Vec<bool> {
    if lines.is_empty() {
        return Vec::new();
    }
    let base = content.as_ptr() as usize;
    lines
        .iter()
        .map(|line| {
            let start = line.as_ptr() as usize;
            let end_idx = start - base + line.len();
            content.get(end_idx) == Some(&b'\n')
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::bytes::Regex as ByteRegex;

    #[test]
    fn replace_single_line() {
        let r = replace_range(b"a\nb\nc\n", 2, 2, b"X").unwrap();
        assert_eq!(r, b"a\nX\nc\n");
    }

    #[test]
    fn replace_range_multi() {
        let r = replace_range(b"a\nb\nc\nd\n", 2, 3, b"X\nY").unwrap();
        assert_eq!(r, b"a\nX\nY\nd\n");
    }

    #[test]
    fn replace_with_empty_deletes_range() {
        let r = replace_range(b"a\nb\nc\nd\n", 2, 3, b"").unwrap();
        assert_eq!(r, b"a\nd\n");
    }

    #[test]
    fn replace_first_line() {
        let r = replace_range(b"a\nb\n", 1, 1, b"X").unwrap();
        assert_eq!(r, b"X\nb\n");
    }

    #[test]
    fn replace_last_line_keeps_trailing_newline() {
        let r = replace_range(b"a\nb\n", 2, 2, b"X").unwrap();
        assert_eq!(r, b"a\nX\n");
    }

    #[test]
    fn replace_last_line_without_trailing_newline_does_not_add_one() {
        let r = replace_range(b"a\nb", 2, 2, b"X").unwrap();
        assert_eq!(r, b"a\nX");
    }

    #[test]
    fn replace_with_payload_having_newline_does_not_double_append() {
        let r = replace_range(b"a\nb\nc\n", 2, 2, b"X\n").unwrap();
        assert_eq!(r, b"a\nX\nc\n");
    }

    #[test]
    fn replace_zero_line_errors() {
        let e = replace_range(b"a\n", 0, 1, b"X").unwrap_err();
        assert!(matches!(e, ReplaceError::ZeroLine));
    }

    #[test]
    fn replace_inverted_range_errors() {
        let e = replace_range(b"a\nb\n", 2, 1, b"X").unwrap_err();
        assert!(matches!(e, ReplaceError::InvalidRange { .. }));
    }

    #[test]
    fn replace_out_of_bounds_errors() {
        let e = replace_range(b"a\n", 1, 5, b"X").unwrap_err();
        assert!(matches!(e, ReplaceError::OutOfBounds { .. }));
    }

    #[test]
    fn pattern_replace_all() {
        let re = ByteRegex::new("foo").unwrap();
        let (out, n) = replace_pattern(b"foo bar foo baz\n", &re, "BAZ", Count::All);
        assert_eq!(out, b"BAZ bar BAZ baz\n");
        assert_eq!(n, 2);
    }

    #[test]
    fn pattern_replace_first_only() {
        let re = ByteRegex::new("foo").unwrap();
        let (out, n) = replace_pattern(b"foo foo foo\n", &re, "X", Count::First(1));
        assert_eq!(out, b"X foo foo\n");
        assert_eq!(n, 1);
    }

    #[test]
    fn pattern_replace_with_capture_group() {
        let re = ByteRegex::new(r"(\w+)=(\d+)").unwrap();
        let (out, n) = replace_pattern(b"x=1 y=22\n", &re, "$2:$1", Count::All);
        assert_eq!(out, b"1:x 22:y\n");
        assert_eq!(n, 2);
    }

    #[test]
    fn pattern_no_match_returns_original() {
        let re = ByteRegex::new("xxx").unwrap();
        let (out, n) = replace_pattern(b"foo\n", &re, "Y", Count::All);
        assert_eq!(out, b"foo\n");
        assert_eq!(n, 0);
    }
}
