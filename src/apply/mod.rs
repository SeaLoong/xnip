//! `apply`：批量编辑指令清单的解析（native / json / yaml）+ 两阶段提交。
//!
//! 数据模型设计：
//! - 三个 parser（`parse_native` / `parse_json` / `parse_yaml`）的共同输出是
//!   `Vec<Op>`，三格式语义等价（PLAN §11 G6）
//! - 执行器 [`commit::execute`] 接管 op 列表，按文件分组、组内降序排序、两阶段提交
//!
//! 详见 PLAN §6.9。

pub mod commit;
pub mod detect;
pub mod parse_json;
pub mod parse_native;
pub mod parse_yaml;

use std::path::PathBuf;

use crate::core::location::Locator;

/// op 的目标文件指定方式。
#[derive(Debug, Clone)]
pub enum Target {
    /// 单文件 `file: <path>`。
    File(PathBuf),
    /// 多文件 `--files-from <path>`：每行一个路径。
    FilesFrom(PathBuf),
}

/// op 携带的内容（按 cli 语义；不含 stdin 计数器，stdin 由执行器消费）。
#[derive(Debug, Clone)]
pub enum OpContent {
    /// 字面字节串。
    Text(Vec<u8>),
    /// 外联文件 `@<path>`。
    File(PathBuf),
    /// `@-`：从 apply 的 stdin 顺序消费一段（在执行器侧实现）。
    Stdin,
    /// `--repl` 替换串（仅 `pattern` 模式）。
    Repl(String),
    /// 删除（等价于 `Text(b"")`，但显式命名便于可读性）。
    Empty,
    /// 不需要内容的 op（如 move）。
    None,
}

impl OpContent {
    pub fn is_none(&self) -> bool {
        matches!(self, OpContent::None)
    }
}

/// 缩进 op 的具体算子（PLAN §6.7.6）。
#[derive(Debug, Clone, Copy)]
pub enum IndentKind {
    /// `+N` / `by: N`（正数 = 加；负数 = 删）。
    Adjust(i64),
    TabsToSpaces(usize),
    SpacesToTabs(usize),
}

/// move op 的目标。
#[derive(Debug, Clone, Copy)]
pub enum MovePosition {
    Before,
    After,
}

/// 内部统一的 op 表示。三个 parser 的目标。
///
/// 不提供 `Clone`。需要复制时使用 [`commit::clone_with_target`] 手动 clone（因
/// `Locator` 含不例年 Clone 的字段设计，避免隐藏 clone 代价）。
#[derive(Debug)]
pub enum Op {
    Replace {
        target: Target,
        locator: Locator,
        content: OpContent,
        was: Option<Vec<u8>>,
        revert: bool,
    },
    Insert {
        target: Target,
        locator: Locator,
        content: OpContent,
        position: MovePosition,
        revert: bool,
    },
    Move {
        target: Target,
        from: Locator,
        to_line: usize,
        position: MovePosition,
        revert: bool,
    },
    Indent {
        target: Target,
        locator: Locator,
        kind: IndentKind,
        revert: bool,
    },
}

impl Op {
    /// 取 op 的目标，便于按文件分组。
    pub fn target(&self) -> &Target {
        match self {
            Op::Replace { target, .. }
            | Op::Insert { target, .. }
            | Op::Move { target, .. }
            | Op::Indent { target, .. } => target,
        }
    }
}
