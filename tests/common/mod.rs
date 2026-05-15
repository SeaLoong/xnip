//! M2 集成测试通用辅助。
//!
//! 部分 helper 仅被一部分集成测试使用（e.g. `read` 只在写命令测试里使用）。
#![allow(dead_code)]

use std::path::PathBuf;
use tempfile::TempDir;

/// 在临时目录创建一个文件，返回 (`TempDir` 持有所有权, 文件路径)。
pub fn tempfile_with(content: &[u8]) -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.txt");
    std::fs::write(&path, content).unwrap();
    (dir, path)
}

/// 读取文件内容。
pub fn read(path: &std::path::Path) -> Vec<u8> {
    std::fs::read(path).unwrap()
}
