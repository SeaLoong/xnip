//! Black-box fixtures: every command uses the same public binary as an agent.
#![allow(dead_code)]

use assert_cmd::Command;
use std::path::{Path, PathBuf};

pub struct Workspace(tempfile::TempDir);

impl Workspace {
    pub fn new(files: &[(&str, &[u8])]) -> Self {
        let workspace = Self(tempfile::tempdir().unwrap());
        for (name, bytes) in files {
            workspace.write(name, bytes);
        }
        workspace
    }

    pub fn root(&self) -> &Path {
        self.0.path()
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.root().join(name)
    }

    pub fn write(&self, name: &str, bytes: &[u8]) {
        let path = self.path(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    pub fn bytes(&self, name: &str) -> Vec<u8> {
        std::fs::read(self.path(name)).unwrap()
    }

    pub fn assert_bytes(&self, name: &str, expected: &[u8]) {
        assert_eq!(self.bytes(name), expected, "unexpected bytes in {name}");
    }

    pub fn absent(&self, name: &str) {
        assert!(!self.path(name).exists(), "unexpected path: {name}");
    }

    pub fn cli(&self, args: &[&str]) -> Command {
        let mut command = Command::cargo_bin("xnip").unwrap();
        command
            .env("XNIP_AUTO_UPDATE", "0")
            .current_dir(self.root())
            .args(args);
        command
    }

    pub fn apply(&self, format: &str, manifest: &str) -> Command {
        let mut command = self.cli(&["apply", "--from-stdin", "--format", format]);
        command.write_stdin(manifest);
        command
    }

    pub fn batch(&self, manifest: &serde_json::Value) -> Command {
        self.apply("json", &manifest.to_string())
    }
}

pub fn utf8(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).unwrap()
}

pub fn json_lines(bytes: &[u8]) -> Vec<serde_json::Value> {
    utf8(bytes)
        .lines()
        .map(|line| serde_json::from_str(line).expect("stdout must contain only JSON"))
        .collect()
}
