//! 4 种 Content source + `@` 外联读取。
//!
//! 写命令的内容来源（互斥，PLAN.md §6.5）：
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

/// 顶层便捷函数（与 `Content::load` 等价；保留以匹配 PLAN.md 命名）。
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn inline_returns_bytes() {
        let c = Content::Inline(b"hello".to_vec());
        assert_eq!(c.load().unwrap(), b"hello");
    }

    #[test]
    fn inline_empty_returns_empty() {
        let c = Content::Inline(vec![]);
        assert_eq!(c.load().unwrap(), b"");
    }

    #[test]
    fn file_returns_bytes() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("c.txt");
        std::fs::write(&p, b"file content").unwrap();
        let c = Content::File(p.clone());
        assert_eq!(c.load().unwrap(), b"file content");
    }

    #[test]
    fn file_missing_returns_read_error() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("missing.txt");
        let c = Content::File(p);
        let err = c.load().unwrap_err();
        matches!(err, ContentError::Read { .. });
    }

    #[test]
    fn replacement_returns_not_byte_content() {
        let c = Content::Replacement("$1".to_string());
        let err = c.load().unwrap_err();
        matches!(err, ContentError::NotByteContent);
        assert_eq!(c.as_replacement(), Some("$1"));
    }

    #[test]
    fn non_replacement_has_no_replacement_str() {
        let c = Content::Inline(b"x".to_vec());
        assert_eq!(c.as_replacement(), None);
    }

    #[test]
    fn load_path_reads_bytes() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("c.txt");
        std::fs::write(&p, b"abc").unwrap();
        assert_eq!(load_path(&p).unwrap(), b"abc");
    }

    #[test]
    fn load_path_missing_yields_read_error() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("missing.txt");
        let err = load_path(&p).unwrap_err();
        matches!(err, ContentError::Read { .. });
    }

    #[test]
    fn bytes_are_transparent_through_inline() {
        let payload: Vec<u8> = vec![0x00, 0xff, 0x01, 0xfe];
        let c = Content::Inline(payload.clone());
        assert_eq!(c.load().unwrap(), payload);
    }
}
