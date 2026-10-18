//! 原子写入：tmpfile 同目录 → fsync → atomic rename；可选 `--backup` 写 `.bak`。
//!
//! 设计要点：
//!
//! - **同目录 tmpfile**：保证 rename 在同一文件系统下原子（POSIX & NTFS）
//! - **fsync**：写入后调用 `sync_all`，避免 crash 后 tmpfile 数据不完整
//! - **可选 `.bak`**：默认不写；`make_bak == true` 时复制到 `bak_path(target)`，
//!   覆盖式（不带 `.bak.1` / `.bak.2`），失败则不进入 rename
//! - **失败安全**：tmpfile drop 时自动删除，不留半成品

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use tempfile::NamedTempFile;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AtomicError {
    #[error("io error during atomic write of {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("target path has no parent directory: {0}")]
    NoParent(PathBuf),
    #[error("backup copy failed for {path}: {source}")]
    Backup {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to persist temp file to {path}: {source}")]
    Persist {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// 原子地把 `new_content` 写入 `target`。
///
/// - 在 `target.parent()` 同目录创建临时文件，保证 rename 原子
/// - 调 `sync_all` 强制刷盘
/// - `make_bak == true` 且 `target` 存在时，先复制为 `bak_path(target)`
/// - 任一步失败：tmpfile 自动删除；若已写出 `.bak` 不主动删除（用户保留）
///
/// # Errors
/// 见 [`AtomicError`]。
pub fn atomic_write(target: &Path, new_content: &[u8], make_bak: bool) -> Result<(), AtomicError> {
    // Follow an existing symlink rather than replacing the link itself.
    let resolved;
    let target = if fs::symlink_metadata(target).is_ok_and(|m| m.file_type().is_symlink()) {
        resolved = fs::canonicalize(target).map_err(|source| AtomicError::Io {
            path: target.to_path_buf(),
            source,
        })?;
        resolved.as_path()
    } else {
        target
    };
    let tmp = prepare_write(target, new_content)?;
    if make_bak && target.exists() {
        backup(target)?;
    }
    tmp.persist(target).map_err(|e| AtomicError::Persist {
        path: target.to_path_buf(),
        source: e.error,
    })?;
    Ok(())
}

/// Stage and sync bytes while preserving existing permissions. Shared by batch writes.
pub fn prepare_write(target: &Path, new_content: &[u8]) -> Result<NamedTempFile, AtomicError> {
    let metadata = match fs::metadata(target) {
        Ok(m) => Some(m),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(source) => {
            return Err(AtomicError::Io {
                path: target.to_path_buf(),
                source,
            });
        }
    };
    if metadata
        .as_ref()
        .is_some_and(|m| m.permissions().readonly())
    {
        return Err(AtomicError::Io {
            path: target.to_path_buf(),
            source: std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "target is read-only",
            ),
        });
    }
    let dir = target
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);

    if !dir.exists() {
        return Err(AtomicError::NoParent(target.to_path_buf()));
    }

    // Phase 1: write tmpfile in same directory
    let mut tmp = NamedTempFile::new_in(&dir).map_err(|e| AtomicError::Io {
        path: target.to_path_buf(),
        source: e,
    })?;
    tmp.write_all(new_content).map_err(|e| AtomicError::Io {
        path: target.to_path_buf(),
        source: e,
    })?;
    if let Some(metadata) = metadata {
        tmp.as_file()
            .set_permissions(metadata.permissions())
            .map_err(|source| AtomicError::Io {
                path: target.to_path_buf(),
                source,
            })?;
    }
    tmp.as_file().sync_all().map_err(|e| AtomicError::Io {
        path: target.to_path_buf(),
        source: e,
    })?;

    Ok(tmp)
}

/// Append `.bak` without discarding the original extension.
pub fn bak_path(target: &Path) -> PathBuf {
    let mut name = target.as_os_str().to_os_string();
    name.push(".bak");
    PathBuf::from(name)
}

/// Stage backups too: never truncate an existing backup or follow its symlink.
pub fn backup(target: &Path) -> Result<(), AtomicError> {
    let bak = bak_path(target);
    if fs::symlink_metadata(&bak).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(AtomicError::Backup {
            path: bak,
            source: std::io::Error::other("backup path is a symlink"),
        });
    }
    let bytes = fs::read(target).map_err(|source| AtomicError::Backup {
        path: target.to_path_buf(),
        source,
    })?;
    let tmp = prepare_write(&bak, &bytes)?;
    tmp.as_file()
        .set_permissions(
            fs::metadata(target)
                .map_err(|source| AtomicError::Io {
                    path: target.to_path_buf(),
                    source,
                })?
                .permissions(),
        )
        .map_err(|source| AtomicError::Io {
            path: bak.clone(),
            source,
        })?;
    tmp.persist(&bak).map_err(|e| AtomicError::Backup {
        path: bak,
        source: e.error,
    })?;
    Ok(())
}
