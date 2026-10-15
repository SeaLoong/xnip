//! `xnip doctor`：自检环境与版本。
//!
//! 检查项（PLAN §7.5 简化）：
//! - xnip 版本与 build commit
//! - 平台（OS / arch）
//! - Rust 编译期 target triple
//! - 当前 cwd 是否可写
//! - stdin / stdout / stderr 是否 TTY
//!
//! 输出格式：人类可读（默认）；JSON（`--json`）后续接入 `output::json`。

use std::io::Write;
use std::sync::OnceLock;

use is_terminal::IsTerminal;

static STDIO_STATUS: OnceLock<[bool; 3]> = OnceLock::new();

/// Capture before a protocol reader starts holding the stdin lock on Windows.
/// The process's standard streams remain unchanged for the server lifetime.
pub(crate) fn capture_stdio_status() -> [bool; 3] {
    *STDIO_STATUS.get_or_init(|| {
        [
            std::io::stdin().is_terminal(),
            std::io::stdout().is_terminal(),
            std::io::stderr().is_terminal(),
        ]
    })
}

/// 把 doctor 报告写到 `out`（通常是 stdout）。
///
/// # Errors
/// 写入失败时返回 IO 错误。
pub fn report<W: Write>(mut out: W) -> std::io::Result<()> {
    writeln!(out, "xnip {} ({})", crate::VERSION, crate::BUILD_COMMIT)?;
    writeln!(out, "  os:       {}", std::env::consts::OS)?;
    writeln!(out, "  arch:     {}", std::env::consts::ARCH)?;
    writeln!(out, "  family:   {}", std::env::consts::FAMILY)?;
    writeln!(out, "  target:   {}", env!("XNIP_TARGET"))?;

    // cwd 可写探针
    match probe_cwd_writable() {
        Ok(true) => writeln!(out, "  cwd-writable: yes")?,
        Ok(false) => writeln!(out, "  cwd-writable: no")?,
        Err(e) => writeln!(out, "  cwd-writable: err ({e})")?,
    }

    let [stdin_tty, stdout_tty, stderr_tty] = capture_stdio_status();
    writeln!(out, "  stdin-tty:  {stdin_tty}")?;
    writeln!(out, "  stdout-tty: {stdout_tty}")?;
    writeln!(out, "  stderr-tty: {stderr_tty}")?;

    Ok(())
}

fn probe_cwd_writable() -> std::io::Result<bool> {
    let cwd = std::env::current_dir()?;
    let probe = tempfile::NamedTempFile::new_in(&cwd);
    Ok(probe.is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_writes_version_and_platform() {
        let mut buf = Vec::new();
        report(&mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.starts_with("xnip "), "report: {s}");
        assert!(s.contains("os:"));
        assert!(s.contains("arch:"));
    }

    #[test]
    fn report_includes_tty_flags() {
        let mut buf = Vec::new();
        report(&mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("stdin-tty:"));
        assert!(s.contains("stdout-tty:"));
        assert!(s.contains("stderr-tty:"));
    }
}
