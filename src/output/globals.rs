//! Process-wide CLI presentation flags, fixed before command dispatch.

use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, Default)]
pub struct Flags {
    /// `--quiet`：压制 stderr 提示（错误仍要输出）。
    pub quiet: bool,
    /// `--no-color`：禁用 ANSI 颜色输出。
    pub no_color: bool,
    /// `--trace`：启用详细 trace 日志到 stderr。
    pub trace: bool,
    /// Presentation for file-edit results.
    pub output: super::human::Output,
}

static FLAGS: OnceLock<Flags> = OnceLock::new();

/// 在 CLI 分发前调用一次；重复调用忽略后续。
pub fn init(flags: Flags) {
    let _ = FLAGS.set(flags);
}

/// 返回运行时冻结的 flags 副本。未 init 时返回默认值（全 false）。
pub fn get() -> Flags {
    FLAGS.get().copied().unwrap_or_default()
}

/// 便捷查询。
#[inline]
#[must_use]
pub fn is_quiet() -> bool {
    get().quiet
}

#[inline]
#[must_use]
pub fn is_no_color() -> bool {
    get().no_color
}

#[inline]
#[must_use]
pub fn trace_enabled() -> bool {
    get().trace
}

/// 非错误性提示：`--quiet` 时不打印。错误仍应直接 `eprintln!`。
#[macro_export]
macro_rules! note {
    ($($arg:tt)*) => {{
        if !$crate::output::globals::is_quiet() {
            eprintln!($($arg)*);
        }
    }};
}

/// Trace 日志：`--trace` 时才打印到 stderr。
#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => {{
        if $crate::output::globals::trace_enabled() {
            eprintln!("[xnip trace] {}", format_args!($($arg)*));
        }
    }};
}
