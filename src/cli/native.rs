//! Explicit profile-scoped native installation and verification.
use std::path::PathBuf;

use clap::{Args as ClapArgs, Subcommand};

use crate::native;

#[derive(serde::Deserialize)]
struct IntegrationCatalog {
    hosts: Vec<Integration>,
}

#[derive(serde::Deserialize)]
struct Integration {
    id: String,
    mode: String,
    scope: String,
}

#[derive(Debug, ClapArgs)]
pub struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Serve local host-adapter requests by calling core transactions over JSON lines.
    Serve,
    /// Install a local adapter and verify the host, rolling back on failure.
    Install(Host),
    /// Restore the installation baseline and verify a real native edit.
    Uninstall(Host),
    /// Recover an interrupted install or removal using retained snapshots.
    Recover(Host),
    /// Check installed or restored configuration, tools and behavior.
    Verify {
        #[command(flatten)]
        host: Host,
        #[arg(long)]
        restored: bool,
    },
    /// Report retained installation state without changing the host.
    Doctor(State),
    /// Purge this installer's private snapshots after verified restoration.
    Clean {
        #[command(flatten)]
        state: State,
        #[arg(long)]
        purge_user_data: bool,
    },
}

#[derive(Debug, ClapArgs)]
struct State {
    #[arg(long)]
    state_dir: PathBuf,
}

#[derive(Debug, ClapArgs)]
struct Host {
    #[arg(long = "host", default_value = "dsh")]
    framework: String,
    /// Existing DSH profile directory, not a profile name.
    #[arg(long)]
    profile: PathBuf,
    /// Adapter directory override. The npm launcher selects its bundled adapter.
    #[arg(long)]
    plugin: Option<PathBuf>,
    #[arg(long)]
    state_dir: Option<PathBuf>,
    #[arg(long, default_value = "node")]
    node: PathBuf,
    #[arg(long, default_value = "dsh")]
    dsh: PathBuf,
}

impl Host {
    fn resolve(self) -> anyhow::Result<(PathBuf, native::CommandLifecycle)> {
        anyhow::ensure!(
            self.framework == "dsh",
            "native profile installation supports dsh; select the host-specific extension or SDK setup for {}",
            self.framework
        );
        let profile = std::fs::canonicalize(self.profile)?;
        let plugin = self
            .plugin
            .or_else(|| std::env::var_os("XNIP_DSH_NATIVE_PLUGIN").map(PathBuf::from))
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "DSH native adapter location is required: use the xnip-cli npm launcher for its bundled adapter, or pass --plugin DIRECTORY"
                )
            })?;
        let plugin = std::fs::canonicalize(&plugin).map_err(|error| {
            anyhow::anyhow!("DSH native adapter directory {}: {error}", plugin.display())
        })?;
        let state = self
            .state_dir
            .unwrap_or_else(|| profile.join(".xnip-native"));
        let state = state_path(&state)?;
        if state.join("install-manifest.json").exists() {
            anyhow::ensure!(
                native::installation_profile(&state)? == profile,
                "selected installation state belongs to another profile"
            );
        }
        let driver = plugin.join("lifecycle.mjs");
        anyhow::ensure!(
            driver.is_file(),
            "native lifecycle driver is missing: {}",
            driver.display()
        );
        Ok((
            state,
            native::CommandLifecycle {
                node: self.node,
                driver,
                profile,
                dsh: resolve_program(&self.dsh)?,
                plugin,
                executable: std::env::current_exe()?,
            },
        ))
    }
}

fn resolve_program(program: &std::path::Path) -> anyhow::Result<PathBuf> {
    if program.is_absolute() || program.components().count() > 1 {
        return Ok(std::fs::canonicalize(program)?);
    }
    if let Some(paths) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&paths) {
            let candidate = directory.join(program);
            if candidate.is_file() {
                return Ok(std::fs::canonicalize(candidate)?);
            }
            #[cfg(windows)]
            {
                let candidate = candidate.with_extension("exe");
                if candidate.is_file() {
                    return Ok(std::fs::canonicalize(candidate)?);
                }
            }
        }
    }
    anyhow::bail!(
        "DSH executable not found: {}; pass --dsh with its path",
        program.display()
    )
}

pub fn run(args: Args) -> anyhow::Result<u8> {
    let result = match args.command {
        Command::Serve => return super::native_backend::run(),
        Command::Install(host) => {
            let (state, mut lifecycle) = host.resolve()?;
            let plan = lifecycle.install_plan(state)?;
            serde_json::to_value(native::install(&plan, &mut lifecycle)?)?
        }
        Command::Uninstall(host) => {
            let (state, mut lifecycle) = host.resolve()?;
            serde_json::to_value(native::uninstall(&state, &mut lifecycle)?)?
        }
        Command::Recover(host) => {
            let (state, mut lifecycle) = host.resolve()?;
            serde_json::to_value(native::recover(&state, &mut lifecycle)?)?
        }
        Command::Verify { host, restored } => {
            let (state, mut lifecycle) = host.resolve()?;
            serde_json::to_value(native::verify(&state, &mut lifecycle, restored)?)?
        }
        Command::Doctor(state) => state_report(&state.state_dir)?,
        Command::Clean {
            state,
            purge_user_data,
        } => serde_json::to_value(native::clean_report(
            &state_path(&state.state_dir)?,
            purge_user_data,
        )?)?,
    };
    let out = std::io::stdout();
    let mut out = out.lock();
    serde_json::to_writer_pretty(&mut out, &result)?;
    std::io::Write::write_all(&mut out, b"\n")?;
    Ok(crate::output::exit::SUCCESS)
}

fn state_path(state: &std::path::Path) -> anyhow::Result<PathBuf> {
    native::resolve_state_path(state)
}

fn state_report(state: &std::path::Path) -> anyhow::Result<serde_json::Value> {
    match native::doctor(&state_path(state)?) {
        Ok(report) => Ok(serde_json::to_value(report)?),
        Err(error) => {
            if let Some(diagnostic) = error.downcast_ref::<native::NativeStateError>() {
                match diagnostic.code {
                    native::NativeStateCode::StateMissing | native::NativeStateCode::StateEmpty => {
                        let mut report = serde_json::to_value(diagnostic)?;
                        report["installed"] = false.into();
                        return Ok(report);
                    }
                    native::NativeStateCode::ManifestMissing => {}
                }
            }
            Err(error)
        }
    }
}

#[derive(Debug, ClapArgs)]
pub struct DoctorArgs {
    /// Report native adapter availability and verification limits.
    #[arg(long)]
    native: bool,
    /// Optional existing native-installation evidence to inspect.
    #[arg(long, requires = "native")]
    state_dir: Option<PathBuf>,
}

pub fn doctor(args: &DoctorArgs) -> anyhow::Result<u8> {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    crate::doctor::report(&mut out)?;
    if args.native {
        use std::io::Write;
        writeln!(out, "  native-protocol: 1 (core transactions)")?;
        if let Some(state) = &args.state_dir {
            serde_json::to_writer_pretty(&mut out, &state_report(state)?)?;
            writeln!(out)?;
        }
        let catalog: IntegrationCatalog =
            serde_json::from_str(include_str!("../../integrations/catalog.json"))?;
        for host in catalog.hosts {
            writeln!(out, "  {}: {}; {}", host.id, host.mode, host.scope)?;
        }
    }
    Ok(crate::output::exit::SUCCESS)
}
