//! Unified diff 生成：基于 `similar` crate。
//!
//! 输出格式（与 `diff -u` 兼容）：
//!
//! ```text
//! --- <path>\t(before)
//! +++ <path>\t(after)
//! @@ -30,3 +30,2 @@
//! -old line 1
//! -old line 2
//! -old line 3
//! +new line
//! ```
//!
//! CLI and MCP use this shared diff representation.

use std::fmt::Write as _;
use std::path::Path;

use similar::{ChangeTag, TextDiff};

// ANSI 颜色序列（git-diff 风格）。
// 仅在 stdout 为 TTY 且未指定 --no-color / NO_COLOR 时使用。
const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m"; // 文件头
const CYAN: &str = "\x1b[36m"; // hunk 头 @@
const RED: &str = "\x1b[31m"; // 删除行
const GREEN: &str = "\x1b[32m"; // 新增行
const DIM: &str = "\x1b[2m"; // \ No newline at end of file

/// 生成 unified diff 文本。
///
/// - 默认上下文 3 行（与 `diff -u` 默认一致）
/// - 输入按 UTF-8 lossy 解码（用于 diff 行级展示）；底层文件仍按字节对待
/// - 文件相同时返回空字符串（无 `---`/`+++` 头）
pub fn unified_diff(path: &Path, before: &[u8], after: &[u8]) -> String {
    unified_diff_with_context(path, before, after, 3)
}

/// 同 [`unified_diff`]，但允许指定 context 行数（仅测试 / 高级用途）。
pub fn unified_diff_with_context(
    path: &Path,
    before: &[u8],
    after: &[u8],
    context_lines: usize,
) -> String {
    if before == after {
        return String::new();
    }

    let before_str = String::from_utf8_lossy(before);
    let after_str = String::from_utf8_lossy(after);
    let diff = TextDiff::from_lines(before_str.as_ref(), after_str.as_ref());

    let mut out = String::new();
    let path_disp = path.display();
    let _ = writeln!(out, "--- {path_disp}\t(before)");
    let _ = writeln!(out, "+++ {path_disp}\t(after)");

    for hunk in diff
        .unified_diff()
        .context_radius(context_lines)
        .iter_hunks()
    {
        let _ = writeln!(out, "{}", hunk.header());
        for change in hunk.iter_changes() {
            let sign = match change.tag() {
                ChangeTag::Delete => '-',
                ChangeTag::Insert => '+',
                ChangeTag::Equal => ' ',
            };
            // change.value() 已包含原行尾；保留之
            let value = change.value();
            out.push(sign);
            out.push_str(value);
            if !value.ends_with('\n') {
                out.push_str("\n\\ No newline at end of file\n");
            }
        }
    }

    out
}

/// 给已生成的 unified diff 文本上色（git-diff 风格 ANSI）。
///
/// 输入应是 [`unified_diff`] / [`unified_diff_with_context`] 的输出；
/// 调用方负责判定环境是否适合上色（见 [`should_colorize_stdout`]）。
///
/// 着色规则（按行首字符）：
/// - `--- ` / `+++ ` → 加粗
/// - `@@`           → 青色
/// - `+`            → 绿色（不含 `+++`）
/// - `-`            → 红色（不含 `---`）
/// - `\ No newline …` → 暗色
/// - 其他           → 不变
#[must_use]
pub fn colorize_unified_diff(plain: &str) -> String {
    let mut out = String::with_capacity(plain.len() + plain.len() / 8);
    for line in plain.split_inclusive('\n') {
        if let Some(rest) = line.strip_prefix("--- ") {
            out.push_str(BOLD);
            out.push_str("--- ");
            out.push_str(rest.trim_end_matches('\n'));
            out.push_str(RESET);
            if line.ends_with('\n') {
                out.push('\n');
            }
        } else if let Some(rest) = line.strip_prefix("+++ ") {
            out.push_str(BOLD);
            out.push_str("+++ ");
            out.push_str(rest.trim_end_matches('\n'));
            out.push_str(RESET);
            if line.ends_with('\n') {
                out.push('\n');
            }
        } else if line.starts_with("@@") {
            out.push_str(CYAN);
            out.push_str(line.trim_end_matches('\n'));
            out.push_str(RESET);
            if line.ends_with('\n') {
                out.push('\n');
            }
        } else if line.starts_with("\\ No newline") {
            out.push_str(DIM);
            out.push_str(line.trim_end_matches('\n'));
            out.push_str(RESET);
            if line.ends_with('\n') {
                out.push('\n');
            }
        } else if line.starts_with('+') {
            out.push_str(GREEN);
            out.push_str(line.trim_end_matches('\n'));
            out.push_str(RESET);
            if line.ends_with('\n') {
                out.push('\n');
            }
        } else if line.starts_with('-') {
            out.push_str(RED);
            out.push_str(line.trim_end_matches('\n'));
            out.push_str(RESET);
            if line.ends_with('\n') {
                out.push('\n');
            }
        } else {
            out.push_str(line);
        }
    }
    out
}

/// 是否应该在 stdout 上输出 ANSI 颜色。
///
/// 综合考虑：
/// 1. 全局 `--no-color` flag（也响应 `NO_COLOR` 环境变量，由 cli 层统一注入到 globals）
/// 2. stdout 必须是 TTY（避免污染 pipe / 文件）
#[must_use]
pub fn should_colorize_stdout() -> bool {
    use is_terminal::IsTerminal;
    !crate::output::globals::is_no_color() && std::io::stdout().is_terminal()
}
