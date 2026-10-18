//! 4 种 Content source + `@` 外联读取。
//!
//! 写命令的内容来源（互斥）：
//! - `--text "..."`：字面字节串（shell 已处理转义）
//! - `--text-stdin`：从 process stdin 整体读取
//! - `--text-file <path>`：从外部文件读取
//! - `--repl <s>`：仅 `--pattern` 模式；支持 `$1` 反向引用（regex crate 原生支持）
//!
//! 注意：apply 原生格式中的 `@<path>` / `@-` / `""` 都先在 parser 层解析为
//! 这里的 `Content::File` / `Content::Stdin` / `Content::Inline(vec![])`。

use std::io::Read;
use std::path::{Path, PathBuf};

use thiserror::Error;

/// 写命令的内容来源（互斥）。
#[derive(Debug, Clone)]
pub enum Content {
    /// `--text "..."` 字面字节串。
    Inline(Vec<u8>),
    /// `--text-stdin`：从 process stdin 整体读取。
    Stdin,
    /// `--text-file <path>` 或原生格式的 `@<path>`。
    File(PathBuf),
    /// `--repl <s>`，仅 `--pattern` 模式。
    Replacement(String),
}

#[derive(Debug, Error)]
pub enum ContentError {
    #[error("failed to read content file {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to read stdin: {0}")]
    Stdin(#[source] std::io::Error),
    #[error("`Content::Replacement` is not byte content; use `as_replacement` instead")]
    NotByteContent,
}

impl Content {
    /// 把内容加载为字节序列。`Replacement` 不算字节内容（应在 `ops::replace` 内部用 regex API 处理）。
    ///
    /// # Errors
    /// 见 [`ContentError`]。
    pub fn load(&self) -> Result<Vec<u8>, ContentError> {
        match self {
            Content::Inline(bytes) => Ok(bytes.clone()),
            Content::Stdin => {
                let mut buf = Vec::new();
                std::io::stdin()
                    .read_to_end(&mut buf)
                    .map_err(ContentError::Stdin)?;
                Ok(buf)
            }
            Content::File(path) => std::fs::read(path).map_err(|e| ContentError::Read {
                path: path.clone(),
                source: e,
            }),
            Content::Replacement(_) => Err(ContentError::NotByteContent),
        }
    }

    /// 取出 `--repl` 字符串。其它来源返回 `None`。
    pub fn as_replacement(&self) -> Option<&str> {
        match self {
            Content::Replacement(s) => Some(s.as_str()),
            _ => None,
        }
    }
}

/// 顶层便捷函数，与 `Content::load` 等价。
///
/// # Errors
/// 见 [`ContentError`]。
pub fn load(content: &Content) -> Result<Vec<u8>, ContentError> {
    content.load()
}

/// 从指定路径加载（避免外部模块直接调 `std::fs::read` 漏掉错误信息上下文）。
///
/// # Errors
/// 见 [`ContentError`]。
pub fn load_path(path: &Path) -> Result<Vec<u8>, ContentError> {
    std::fs::read(path).map_err(|e| ContentError::Read {
        path: path.to_path_buf(),
        source: e,
    })
}
