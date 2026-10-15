//! xnip binary entry point and error-to-exit-code mapping.

use std::process::ExitCode;

fn main() -> ExitCode {
    match xnip::cli::run(std::env::args_os()) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("xnip: {e:#}");
            let io = e.chain().any(<dyn std::error::Error>::is::<std::io::Error>);
            ExitCode::from(if io {
                xnip::output::exit::IO
            } else {
                xnip::output::exit::USAGE
            })
        }
    }
}
