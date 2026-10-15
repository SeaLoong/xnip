//! 原子写入：tmpfile 同目录 → fsync → atomic rename；可选 `--backup` 写 `.bak`。
//!
//! 设计要点（PLAN.md §6.1 / §7.3.2）：
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn writes_new_file_when_missing() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("a.txt");
        atomic_write(&target, b"hello", false).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"hello");
        assert!(!bak_path(&target).exists());
    }

    #[test]
    fn overwrites_existing_file() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("a.txt");
        fs::write(&target, b"old").unwrap();
        atomic_write(&target, b"new", false).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert!(!bak_path(&target).exists());
    }

    #[test]
    fn writes_bak_when_requested_and_existing() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("a.txt");
        fs::write(&target, b"old").unwrap();
        atomic_write(&target, b"new", true).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert_eq!(fs::read(bak_path(&target)).unwrap(), b"old");
    }

    #[test]
    fn no_bak_when_target_missing_even_if_requested() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("a.txt");
        atomic_write(&target, b"new", true).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert!(!bak_path(&target).exists());
    }

    #[test]
    fn bak_overwrites_previous_bak() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("a.txt");
        fs::write(&target, b"v1").unwrap();
        atomic_write(&target, b"v2", true).unwrap();
        assert_eq!(fs::read(bak_path(&target)).unwrap(), b"v1");
        atomic_write(&target, b"v3", true).unwrap();
        assert_eq!(fs::read(bak_path(&target)).unwrap(), b"v2");
    }

    #[test]
    fn bytes_are_transparent_including_nul() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("a.bin");
        let payload = b"\x00\xff\x01\xfe utf8: \xe4\xbd\xa0\xe5\xa5\xbd";
        atomic_write(&target, payload, false).unwrap();
        assert_eq!(fs::read(&target).unwrap(), payload);
    }

    #[test]
    fn relative_path_in_cwd_works() {
        // Avoid mutating the process-wide working directory in a parallel test suite.
        let cwd = std::env::current_dir().unwrap();
        let dir = tempfile::tempdir_in(&cwd).unwrap();
        let target = dir.path().join("rel.txt");
        let relative = target.strip_prefix(&cwd).unwrap();
        atomic_write(relative, b"x", false).unwrap();
        assert_eq!(fs::read(target).unwrap(), b"x");
    }
}
