//! xnip 二进制入口。
//!
//! M0 阶段：解析 `--version` / `--help` / 子命令名，未实现的子命令统一退 `EXIT_USAGE`
//! 并在 stderr 提示 "not yet implemented in this milestone"。
//!
//! 真正的 CLI 解析与分发在 M2/M3/M4 逐步填充 `xnip::cli::run`。

use std::process::ExitCode;

fn main() -> ExitCode {
    match xnip::cli::run(std::env::args_os()) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("xnip: {e}");
            ExitCode::from(xnip::output::exit::USAGE)
        }
    }
}
