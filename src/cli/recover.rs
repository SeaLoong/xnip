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
        crate::output::human::print_diff(&report.diff)?;
    }
    let mut summary = crate::output::human::ChangeSummary::batch(
        "recover",
        crate::output::human::Mode::new(args.dry_run, args.check),
        &report,
    );
    summary.journal = Some(args.journal.to_string_lossy().into_owned());
    crate::note!("{}", summary.render());
    Ok(crate::output::exit::SUCCESS)
}
