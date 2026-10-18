//! Build, platform, filesystem and stdio diagnostics.

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
    #[test]
    fn diagnostic_reports_the_running_build_and_stream_capabilities() {
        let mut bytes = Vec::new();
        super::report(&mut bytes).unwrap();
        let report = String::from_utf8(bytes).unwrap();
        for expected in [
            crate::VERSION,
            crate::BUILD_COMMIT,
            std::env::consts::OS,
            std::env::consts::ARCH,
            env!("XNIP_TARGET"),
            "cwd-writable:",
            "stdin-tty:",
            "stdout-tty:",
            "stderr-tty:",
        ] {
            assert!(report.contains(expected), "missing {expected}: {report}");
        }
    }
}
