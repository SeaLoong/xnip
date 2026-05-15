//! 参数对称反向计算（`--revert` 实现）。
//!
//! 设计原则（PLAN.md §6.8）：「同参数加 `--revert` 与不加完全互逆。不可逆即报错。」
//!
//! 本模块提供两类工具：
//! 1. `RevertError` 枚举：各 op 在 revert 前置条件不满足时统一返回的错误
//! 2. `Direction`：op 的执行方向，op 实现里调 `direction.is_revert()` 决定语义
//!
//! 真正的反向计算（如 `move` 的 source/target 自动算反、`indent` 的 N 取反）
//! 在各 `core/ops/<op>.rs` 中就近实现，而非集中在本模块。
//!
//! 这样的拆分理由：
//! - revert 的可逆性条件与具体 op 的语义紧耦合
//! - 集中实现会变成 7 个 op 的大 match，可读性差
//! - 各 op 同时 own forward 和 reverse 路径，更易维护一致

use thiserror::Error;

#[derive(Debug, Error, Eq, PartialEq)]
pub enum RevertError {
    /// 参数本身不携带可逆信息（如 `replace --pattern` 无 `--was`）。
    #[error("revert not possible: {0}")]
    NotInvertible(String),

    /// 不严格可逆（如 `tabs-to-spaces N` 区域内有非 N 倍数的连续空格）。
    #[error("revert not strictly invertible: {0}")]
    NotStrictlyInvertible(String),

    /// revert 的前置条件不匹配（如 `insert --revert` 时目标行段实际不等于待删内容）。
    #[error("revert pre-condition mismatch: {0}")]
    PreconditionMismatch(String),
}

/// op 的执行方向。`Direction::is_revert()` 用于在 op 实现里分支。
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Direction {
    Forward,
    Revert,
}

impl Direction {
    pub fn from_flag(revert: bool) -> Self {
        if revert {
            Direction::Revert
        } else {
            Direction::Forward
        }
    }

    pub fn is_revert(self) -> bool {
        matches!(self, Direction::Revert)
    }
}

/// 工具函数：把 `replace --pattern A --repl B --revert` 反向为 `--pattern B --repl A`。
///
/// 这是 PLAN.md §6.8 表里描述的最简单 case，不涉及 `--was` 校验。
/// 调用方需自行处理 regex 的特殊字符转义（pattern 字段不再 take 字面 B 而是 `regex::escape(B)`）。
pub fn invert_pattern_replacement(pattern: &str, repl: &str) -> (String, String) {
    (regex::escape(repl), pattern.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direction_from_flag_true_is_revert() {
        assert_eq!(Direction::from_flag(true), Direction::Revert);
        assert!(Direction::from_flag(true).is_revert());
    }

    #[test]
    fn direction_from_flag_false_is_forward() {
        assert_eq!(Direction::from_flag(false), Direction::Forward);
        assert!(!Direction::from_flag(false).is_revert());
    }

    #[test]
    fn invert_pattern_swaps_and_escapes() {
        let (p, r) = invert_pattern_replacement("OLD_NAME", "NEW.NAME");
        // NEW.NAME 中的 . 被 escape；旧 pattern 作为新 replacement 字面化
        assert_eq!(p, r"NEW\.NAME");
        assert_eq!(r, "OLD_NAME");
    }

    #[test]
    fn invert_pattern_double_invert_round_trips_for_safe_strings() {
        // 对 [A-Za-z_] 这种 regex-safe 串，二次反转回到原输入
        let (p1, r1) = invert_pattern_replacement("OLD", "NEW");
        let (p2, r2) = invert_pattern_replacement(&p1, &r1);
        assert_eq!(p2, "OLD");
        assert_eq!(r2, "NEW");
    }

    #[test]
    fn revert_error_messages_are_distinct() {
        let e1 = RevertError::NotInvertible("a".into());
        let e2 = RevertError::NotStrictlyInvertible("a".into());
        let e3 = RevertError::PreconditionMismatch("a".into());
        assert_ne!(format!("{e1}"), format!("{e2}"));
        assert_ne!(format!("{e2}"), format!("{e3}"));
    }
}
