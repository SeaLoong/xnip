//! \u5168\u5c40 CLI flag \u7684\u8fd0\u884c\u65f6\u8bbf\u95ee\u3002
//!
//! 设计：`Cli::run` 在分发前调用 `init(flags)` 把用户传入的 `--quiet/--no-color/--trace`
//! 冻结到 `OnceLock` 里；各命令读取 `get()` 获取只读视图。
//!
//! 选择 `OnceLock` 而非线程局部：apply 阶段一可能并行处理，必须用全局只读状态。
//!
//! 无额外依赖：trace 宏直接 `eprintln!`，颜色判定返回 bool 供调用方绕过 ANSI 转义。

use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, Default)]
pub struct Flags {
    /// `--quiet`：压制 stderr 提示（错误仍要输出）。
    pub quiet: bool,
    /// `--no-color`：禁用 ANSI 颜色输出。
    pub no_color: bool,
    /// `--trace`：启用详细 trace 日志到 stderr。
    pub trace: bool,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_flags_are_all_false() {
        // 未 init 时 get() 返回默认值
        let f = Flags::default();
        assert!(!f.quiet);
        assert!(!f.no_color);
        assert!(!f.trace);
    }

    #[test]
    fn flags_copy() {
        let f = Flags {
            quiet: true,
            no_color: true,
            trace: true,
        };
        let g = f;
        assert!(g.quiet && g.no_color && g.trace);
    }
}
