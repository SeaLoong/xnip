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

use is_terminal::IsTerminal;

/// 把 doctor 报告写到 `out`（通常是 stdout）。
///
/// # Errors
/// 写入失败时返回 IO 错误。
pub fn report<W: Write>(mut out: W) -> std::io::Result<()> {
    writeln!(out, "xnip {} ({})", crate::VERSION, crate::BUILD_COMMIT)?;
    writeln!(out, "  os:       {}", std::env::consts::OS)?;
    writeln!(out, "  arch:     {}", std::env::consts::ARCH)?;
    writeln!(out, "  family:   {}", std::env::consts::FAMILY)?;
    writeln!(
        out,
        "  target:   {}",
        option_env!("TARGET").unwrap_or("unknown")
    )?;

    // cwd 可写探针
    match probe_cwd_writable() {
        Ok(true) => writeln!(out, "  cwd-writable: yes")?,
        Ok(false) => writeln!(out, "  cwd-writable: no")?,
        Err(e) => writeln!(out, "  cwd-writable: err ({e})")?,
    }

    writeln!(out, "  stdin-tty:  {}", std::io::stdin().is_terminal())?;
    writeln!(out, "  stdout-tty: {}", std::io::stdout().is_terminal())?;
    writeln!(out, "  stderr-tty: {}", std::io::stderr().is_terminal())?;

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
