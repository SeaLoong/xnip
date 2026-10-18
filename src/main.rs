//! xnip binary entry point and error-to-exit-code mapping.

use std::process::ExitCode;

fn main() -> ExitCode {
    if std::env::args_os()
        .nth(1)
        .is_some_and(|value| value == xnip::update::WORKER_COMMAND)
    {
        xnip::update::worker_entry();
        return ExitCode::SUCCESS;
    }
    match xnip::cli::run(std::env::args_os()) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("xnip: {e:#}");
            let io = e.chain().any(|cause| {
                cause.is::<std::io::Error>()
                    || cause.is::<xnip::core::atomic::AtomicError>()
                    || cause
                        .downcast_ref::<xnip::core::edit::EditError>()
                        .is_some_and(|edit| {
                            matches!(
                                edit,
                                xnip::core::edit::EditError::Io(_)
                                    | xnip::core::edit::EditError::Atomic(_)
                            )
                        })
            });
            ExitCode::from(if io {
                xnip::output::exit::IO
            } else {
                xnip::output::exit::USAGE
            })
        }
    }
}
