//! `replace`：替换/删除（空文本即删除）。
//!
//! 两套核心路径：
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
        Count::First(0) => return (content.to_vec(), 0),
        Count::First(n) => n,
    };
    if !replacement.contains('$') {
        let mut out = Vec::with_capacity(content.len());
        let mut cursor = 0;
        let mut replaced = 0;
        for hit in re
            .find_iter(content)
            .take(if limit == 0 { usize::MAX } else { limit })
        {
            out.extend_from_slice(&content[cursor..hit.start()]);
            out.extend_from_slice(replacement.as_bytes());
            cursor = hit.end();
            replaced += 1;
        }
        out.extend_from_slice(&content[cursor..]);
        return (out, replaced);
    }
    let mut n_replaced = 0;
    let new_bytes = re
        .replacen(content, limit, |caps: &regex::bytes::Captures<'_>| {
            n_replaced += 1;
            let mut expanded = Vec::new();
            caps.expand(replacement.as_bytes(), &mut expanded);
            expanded
        })
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
