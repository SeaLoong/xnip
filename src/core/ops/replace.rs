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

use crate::core::location::Count;

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

    let mut line = 1;
    let mut line_start = 0;
    let mut start = 0;
    for newline in memchr::memchr_iter(b'\n', content) {
        if line == start_line {
            start = line_start;
        }
        if line == end_line {
            return Ok(splice(content, start, newline + 1, payload));
        }
        line += 1;
        line_start = newline + 1;
    }
    if line_start < content.len() {
        if line == start_line {
            start = line_start;
        }
        if line == end_line {
            return Ok(splice(content, start, content.len(), payload));
        }
    }
    let total = line - usize::from(line_start == content.len());
    Err(ReplaceError::OutOfBounds {
        start: start_line,
        end: end_line,
        total,
    })
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

fn splice(content: &[u8], start: usize, end: usize, payload: &[u8]) -> Vec<u8> {
    let newline = !payload.is_empty()
        && content.get(end.wrapping_sub(1)) == Some(&b'\n')
        && payload.last() != Some(&b'\n');
    let mut out =
        Vec::with_capacity(content.len() - (end - start) + payload.len() + usize::from(newline));
    out.extend_from_slice(&content[..start]);
    out.extend_from_slice(payload);
    if newline {
        out.push(b'\n');
    }
    out.extend_from_slice(&content[end..]);
    out
}
