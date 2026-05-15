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
//! 详见 PLAN.md §6.7.3。

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn identical_yields_empty_string() {
        let s = unified_diff(Path::new("x.txt"), b"a\nb\n", b"a\nb\n");
        assert!(s.is_empty(), "expected empty diff, got: {s:?}");
    }

    #[test]
    fn header_uses_path() {
        let s = unified_diff(Path::new("foo/bar.txt"), b"a\n", b"b\n");
        assert!(s.contains("--- foo/bar.txt\t(before)"));
        assert!(s.contains("+++ foo/bar.txt\t(after)"));
    }

    #[test]
    fn delete_line_shows_minus() {
        let s = unified_diff(Path::new("x"), b"a\nb\nc\n", b"a\nc\n");
        assert!(s.contains("-b"), "diff:\n{s}");
        assert!(!s.contains("+b"), "diff:\n{s}");
    }

    #[test]
    fn insert_line_shows_plus() {
        let s = unified_diff(Path::new("x"), b"a\nc\n", b"a\nb\nc\n");
        assert!(s.contains("+b"), "diff:\n{s}");
    }

    #[test]
    fn replace_line_shows_minus_then_plus() {
        let s = unified_diff(Path::new("x"), b"a\n", b"b\n");
        let m = s.find("-a").expect("expected -a");
        let p = s.find("+b").expect("expected +b");
        assert!(m < p, "minus should come before plus, diff:\n{s}");
    }

    #[test]
    fn no_newline_at_eof_is_marked() {
        let s = unified_diff(Path::new("x"), b"a", b"b");
        assert!(s.contains("\\ No newline at end of file"), "diff:\n{s}");
    }

    #[test]
    fn hunk_header_format() {
        let s = unified_diff(Path::new("x"), b"a\n", b"b\n");
        assert!(s.contains("@@ "), "expected hunk header, diff:\n{s}");
    }

    #[test]
    fn colorize_wraps_minus_and_plus_lines() {
        let plain = "\
--- a\t(before)
+++ a\t(after)
@@ -1,1 +1,1 @@
-old
+new
";
        let c = colorize_unified_diff(plain);
        // 关键 ANSI 序列存在
        assert!(c.contains("\x1b[1m--- a"), "header bold: {c:?}");
        assert!(c.contains("\x1b[1m+++ a"), "header bold: {c:?}");
        assert!(c.contains("\x1b[36m@@"), "hunk cyan: {c:?}");
        assert!(c.contains("\x1b[31m-old"), "del red: {c:?}");
        assert!(c.contains("\x1b[32m+new"), "ins green: {c:?}");
        // RESET 后跟换行（保证下游 pager 不会带色至下一行）
        assert!(c.contains("\x1b[0m\n"), "reset before newline: {c:?}");
    }

    #[test]
    fn colorize_does_not_mistake_triple_dash_or_plus_as_change_lines() {
        // `--- ` / `+++ ` 必须走 BOLD 路径，不走 RED/GREEN
        let plain = "--- a\t(before)\n+++ a\t(after)\n";
        let c = colorize_unified_diff(plain);
        assert!(!c.contains("\x1b[31m"), "must not redden ---: {c:?}");
        assert!(!c.contains("\x1b[32m"), "must not green +++: {c:?}");
    }

    #[test]
    fn colorize_marks_no_newline_marker_dim() {
        let plain = "@@ -1 +1 @@\n-a\n\\ No newline at end of file\n+b\n";
        let c = colorize_unified_diff(plain);
        assert!(
            c.contains("\x1b[2m\\ No newline at end of file"),
            "dim marker: {c:?}"
        );
    }

    #[test]
    fn colorize_preserves_content_byte_count_minus_ansi() {
        // 去掉 ANSI 后应等于原 plain
        let plain = unified_diff(Path::new("x"), b"a\nb\nc\n", b"a\nB\nc\n");
        let c = colorize_unified_diff(&plain);
        let stripped = strip_ansi(&c);
        assert_eq!(stripped, plain);
    }

    fn strip_ansi(s: &str) -> String {
        // 极简 strip：仅处理 \x1b[<digits>(;<digits>)*m
        let mut out = String::with_capacity(s.len());
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\x1b' && chars.peek() == Some(&'[') {
                chars.next(); // [
                while let Some(&n) = chars.peek() {
                    chars.next();
                    if n == 'm' {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }
}
