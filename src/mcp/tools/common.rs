//! MCP tools 共享辅助：
//! - [`LocatorInput`] / [`ContentInput`]：JsonSchema 输入结构，可转换为 cli 层的
//!   [`crate::cli::common::LocatorArgs`] / [`crate::cli::common::ContentArgs`]
//! - 错误转换：[`anyhow::Error`] / 各种 core 错误 → [`McpError`]
//! - 输出助手：把 `String` / `Vec<u8>` 包装成 [`CallToolResult`]
#![allow(clippy::doc_markdown)] // intra-doc link 不需要反引号

use std::path::PathBuf;

use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, Content},
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::cli::common::{ContentArgs, LocatorArgs};

// ---------- Locator 输入 ---------- //

/// 与 cli `LocatorArgs` 同义的 5 维定位输入。字段名与 cli `--lines` / `--match-line` / ...
/// 一一对应，方便 LLM 理解。
///
/// 互斥规则：5 个 locator 字段（`lines` / `match_line` / `between` / `between_re` / `pattern`）
/// 必须有且仅有 1 个非空。校验委托给 `LocatorArgs::into_locator()`。
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct LocatorInput {
    /// 行号区间字符串，1-based 闭区间。例：`"30"` 单行 / `"30-45"` 区间。
    #[serde(default)]
    pub lines: Option<String>,

    /// 整行匹配正则。例：`"^const PORT"`。
    #[serde(default)]
    pub match_line: Option<String>,

    /// `match_line` / `between` / `between_re` 的命中序号（1-based），默认 1。
    #[serde(default)]
    pub occurrence: Option<usize>,

    /// 字面锚点对，格式 `"START..END"`。例：`"// BEGIN..// END"`。
    #[serde(default)]
    pub between: Option<String>,

    /// 正则锚点对，格式 `"START..END"`。例：`"^function foo..^\\}"`。
    #[serde(default)]
    pub between_re: Option<String>,

    /// `between` / `between_re` 是否包含锚点行（默认 false）。
    #[serde(default)]
    pub inclusive: Option<bool>,

    /// 跨行字节级正则（仅 `replace` 工具支持，其它工具传此字段会被拒绝）。
    #[serde(default)]
    pub pattern: Option<String>,

    /// `pattern` 模式的命中数：`"all"` 或正整数。默认 `"all"`。
    #[serde(default)]
    pub count: Option<String>,
}

impl LocatorInput {
    /// 转成 cli 层的 `LocatorArgs` 实例（不消费 self，因为字段都是 owned，需要 clone）。
    fn build_locator_args(&self) -> LocatorArgs {
        LocatorArgs {
            lines: self.lines.clone(),
            match_line: self.match_line.clone(),
            occurrence: self.occurrence.unwrap_or(1).max(1),
            between: self.between.clone(),
            between_re: self.between_re.clone(),
            inclusive: self.inclusive.unwrap_or(false),
            pattern: self.pattern.clone(),
            count: self.count.clone(),
        }
    }

    /// 直接转 [`crate::core::location::Locator`]。
    pub fn into_locator(self) -> Result<crate::core::location::Locator, McpError> {
        self.build_locator_args()
            .into_locator()
            .map_err(any_to_invalid_params)
    }
}

// ---------- Content 输入 ---------- //

/// 与 cli `ContentArgs` 同义的内容来源输入。
///
/// **不暴露 `text_stdin`**：MCP 进程的 stdin 已被协议占用，从其读字节会破坏会话。
/// LLM 需要塞内容时直接用 `text`（小内容）或 `text_file`（大内容）。
///
/// 互斥规则：`text` / `text_file` / `repl` 至多 1 个非空；`replace` 命令必须有 1 个，
/// 其它写命令必须有 1 个但不能是 `repl`。
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ContentInput {
    /// 字面字符串（适合短内容）。
    #[serde(default)]
    pub text: Option<String>,

    /// 从外部文件读取（适合大段内容）。
    #[serde(default)]
    pub text_file: Option<PathBuf>,

    /// 替换字符串（仅 `replace` + `pattern` 模式可用，支持 `$1` 反向引用）。
    #[serde(default)]
    pub repl: Option<String>,
}

impl ContentInput {
    fn build_content_args(self) -> ContentArgs {
        ContentArgs {
            text: self.text,
            text_stdin: false,
            text_file: self.text_file,
            repl: self.repl,
        }
    }

    /// 转 [`crate::core::content::Content`]；`require_some=true` 时缺失内容报错。
    pub fn into_content(
        self,
        require_some: bool,
    ) -> Result<Option<crate::core::content::Content>, McpError> {
        self.build_content_args()
            .into_content(require_some)
            .map_err(any_to_invalid_params)
    }
}

// ---------- 错误转换 ---------- //

/// `anyhow::Error` 多用于参数解析/校验失败 → `invalid_params`。
pub fn any_to_invalid_params(e: anyhow::Error) -> McpError {
    McpError::invalid_params(format!("{e}"), None)
}

/// 定位失败 / 状态前提不满足 → `invalid_request`。
pub fn locate_to_mcp_err(e: crate::core::location::LocateError) -> McpError {
    McpError::invalid_request(format!("location error: {e}"), None)
}

/// IO 失败 → `internal_error`。
pub fn io_to_mcp_err(e: std::io::Error) -> McpError {
    McpError::internal_error(format!("io error: {e}"), None)
}

/// 通用：把任何 `Display` 错误映射为 `internal_error`。
pub fn err_to_internal<E: std::fmt::Display>(e: E) -> McpError {
    McpError::internal_error(format!("{e}"), None)
}

// ---------- 输出助手 ---------- //

/// 用单段文本构造成功结果。
pub fn ok_text(s: impl Into<String>) -> CallToolResult {
    CallToolResult::success(vec![Content::text(s.into())])
}

/// 用字节切片当 UTF-8 文本（非法 UTF-8 用 `from_utf8_lossy`）构造成功结果。
pub fn ok_bytes_as_text(bytes: &[u8]) -> CallToolResult {
    CallToolResult::success(vec![Content::text(
        String::from_utf8_lossy(bytes).into_owned(),
    )])
}
