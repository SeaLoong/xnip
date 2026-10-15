use anyhow::Result;
use clap::Args as ClapArgs;
use std::path::PathBuf;

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Recovery directory printed by a failed transaction.
    pub journal: PathBuf,
    /// Validate snapshots and conflicts without restoring files.
    #[arg(long, conflicts_with = "dry_run")]
    pub check: bool,
    /// Preview rollback without restoring files.
    #[arg(long)]
    pub dry_run: bool,
}
pub fn run(args: &Args) -> Result<u8> {
    let report = match crate::core::transaction::recover(&args.journal, args.check || args.dry_run)
    {
        Ok(report) => report,
        Err(error) => {
            eprintln!("xnip recover: {error:#}");
            return Ok(crate::output::exit::CHECK);
        }
    };
    if args.dry_run {
        use std::io::Write;
        std::io::stdout().lock().write_all(report.diff.as_bytes())?;
    }
    crate::note!(
        "xnip recover: {} {} file(s); snapshots retained at {}",
        if args.check || args.dry_run {
            "validated"
        } else {
            "restored"
        },
        report.files.len(),
        args.journal.display()
    );
    Ok(crate::output::exit::SUCCESS)
}
