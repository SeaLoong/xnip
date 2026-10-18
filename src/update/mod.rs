//! Silent background release updates. Foreground calls only register local usage and spawn.
//! An OS file lock serializes workers across all harnesses sharing an installation.
pub mod delivery;
pub mod policy;

use anyhow::{Context, Result, bail};
use delivery::DeliveryConfig;
use policy::{Lease, Pending, State, Usage};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const WORKER_COMMAND: &str = "__update-worker";
const FAILURE_RETRY_SECS: u64 = 300;
const MAX_STATE_BYTES: u64 = 64 * 1024;

#[derive(Clone, Debug)]
pub struct Config {
    pub executable: PathBuf,
    pub directory: PathBuf,
    pub idle_secs: u64,
    pub max_defer_secs: u64,
    pub check_interval_secs: u64,
    pub poll_secs: u64,
    pub delivery: DeliveryConfig,
}

impl Config {
    pub fn from_env() -> Result<Option<Self>> {
        if std::env::var_os("XNIP_AUTO_UPDATE").is_some_and(|value| value == "0") {
            return Ok(None);
        }
        let executable = fs::canonicalize(std::env::current_exe()?)?;
        let base = if let Some(path) = std::env::var_os("XNIP_UPDATE_STATE_DIR") {
            PathBuf::from(path)
        } else {
            state_home()?.join("xnip").join("update")
        };
        let identity = executable.to_string_lossy();
        #[cfg(windows)]
        let identity = identity.to_lowercase();
        let hash = format!("{:x}", Sha256::digest(identity.as_bytes()));
        let mut delivery = DeliveryConfig::default();
        if let Ok(proxy) = std::env::var("XNIP_UPDATE_PROXY") {
            delivery.proxy = delivery::normalize_proxy(&proxy)?;
        }
        if cfg!(debug_assertions)
            && let Ok(url) = std::env::var("XNIP_UPDATE_RELEASE_URL")
        {
            delivery.release_url = url;
            delivery.proxy = None;
        }
        Ok(Some(Self {
            executable,
            directory: base.join(&hash[..24]),
            idle_secs: env_seconds("XNIP_UPDATE_IDLE_SECS", 30 * 60),
            max_defer_secs: env_seconds("XNIP_UPDATE_MAX_DEFER_SECS", 24 * 60 * 60),
            check_interval_secs: env_seconds("XNIP_UPDATE_CHECK_INTERVAL_SECS", 6 * 60 * 60),
            poll_secs: env_seconds("XNIP_UPDATE_POLL_SECS", 30),
            delivery,
        }))
    }
}

fn env_seconds(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value: &u64| *value > 0)
        .unwrap_or(default)
}

fn state_home() -> Result<PathBuf> {
    #[cfg(windows)]
    if let Some(path) = std::env::var_os("LOCALAPPDATA") {
        return Ok(path.into());
    }
    #[cfg(not(windows))]
    if let Some(path) = std::env::var_os("XDG_STATE_HOME") {
        return Ok(path.into());
    }
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .context("no per-user update directory")?;
    Ok(PathBuf::from(home).join(".local").join("state"))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn create_private_dir(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)?;
    }
    #[cfg(not(unix))]
    fs::create_dir_all(path)?;
    if fs::symlink_metadata(path)?.file_type().is_symlink() {
        bail!("update directory is a link");
    }
    Ok(())
}

/// Permanent lock files are never unlinked, avoiding locks on different inodes.
fn lock(path: &Path) -> Result<Option<File>> {
    if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        bail!("update lock is a link");
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(std::fs::TryLockError::Error(error)) => Err(error.into()),
    }
}

#[derive(Clone, Debug)]
pub struct Coordinator {
    pub config: Config,
}

impl Coordinator {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    fn initialize(&self) -> Result<()> {
        create_private_dir(&self.config.directory)?;
        create_private_dir(&self.config.directory.join("leases"))?;
        Ok(())
    }

    fn gate(&self) -> Result<Option<File>> {
        lock(&self.config.directory.join("state.lock"))
    }

    pub fn read_state(&self) -> Result<State> {
        let path = self.config.directory.join("state.json");
        if fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            bail!("update state is a link");
        }
        let file = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(State::default());
            }
            Err(error) => return Err(error.into()),
        };
        let mut bytes = Vec::new();
        file.take(MAX_STATE_BYTES + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_STATE_BYTES {
            bail!("update state exceeds size limit");
        }
        Ok(
            serde_json::from_slice::<State>(&bytes).unwrap_or_else(|_| State {
                phase: "state_reset".into(),
                last_error: Some("invalid state reset".into()),
                ..State::default()
            }),
        )
    }

    fn save(&self, state: &State) -> Result<()> {
        let bytes = serde_json::to_vec(state)?;
        let mut temporary = tempfile::NamedTempFile::new_in(&self.config.directory)?;
        temporary.write_all(&bytes)?;
        // State is advisory scheduling data. Atomic persistence avoids torn JSON;
        // foreground registration does not wait for a disk flush or another reader.
        let target = self.config.directory.join("state.json");
        #[cfg(windows)]
        match fs::symlink_metadata(&target) {
            Ok(metadata) => {
                if !metadata.file_type().is_file() {
                    bail!("update state is not a regular file");
                }
                // MoveFileEx (used by persist) cannot replace an open destination,
                // even when its reader shares deletion. ReplaceFile can, provided
                // the staging handle is closed before Windows opens it exclusively.
                let staged = temporary.into_temp_path();
                let target = target.to_str().context("non-Unicode update state path")?;
                let staged_path = staged.to_str().context("non-Unicode staged state path")?;
                winsafe::SetFileAttributes(staged_path, winsafe::co::FILE_ATTRIBUTE::NORMAL)?;
                winsafe::ReplaceFile(
                    target,
                    staged_path,
                    None,
                    winsafe::co::REPLACEFILE::default(),
                )
                .context("cannot replace update state")?;
                return Ok(());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        temporary.persist(target)?;
        Ok(())
    }

    /// No network IO or waiting for another process. A busy gate is conservative:
    /// generation zero blocks an idle-start install until this usage has ended.
    pub fn begin(&self, kind: Usage, timestamp: u64) -> Result<Session> {
        self.initialize()?;
        let gate = self.gate()?;
        let mut state = if gate.is_some() {
            self.read_state()?
        } else {
            State::default()
        };
        let generation = if gate.is_some() {
            state.register_usage(kind, timestamp, self.config.idle_secs)
        } else {
            0
        };
        // Lock outside the scanned directory, then publish the lease atomically.
        // A worker must never mistake a not-yet-locked new lease for a dead process.
        let mut named = tempfile::Builder::new()
            .prefix("session-")
            .suffix(".json")
            .tempfile_in(&self.config.directory)?;
        named.as_file().try_lock()?;
        let path = self
            .config
            .directory
            .join("leases")
            .join(named.path().file_name().context("lease has no filename")?);
        let lease = Lease {
            process_id: std::process::id(),
            kind,
            generation,
            activity_at: timestamp,
        };
        serde_json::to_writer(named.as_file_mut(), &lease)?;
        fs::write(path.with_extension("meta"), serde_json::to_vec(&lease)?)?;
        let file = named.persist_noclobber(&path)?;
        let mut start_worker = gate.is_none() && kind == Usage::Mcp;
        if gate.is_some() {
            let needed =
                kind == Usage::Mcp || state.pending.is_some() || timestamp >= state.next_check_at;
            let cooldown = state
                .last_spawn_at
                .is_none_or(|last| timestamp.saturating_sub(last) >= 3);
            if needed && cooldown && lock(&self.config.directory.join("worker.lock"))?.is_some() {
                state.last_spawn_at = Some(timestamp);
                start_worker = true;
            }
            self.save(&state)?;
        }
        Ok(Session {
            file,
            lease,
            path,
            start_worker,
            coordinator: self.clone(),
        })
    }

    fn sweep(&self, state: &mut State, timestamp: u64) -> Result<Vec<Lease>> {
        let mut live = Vec::new();
        for entry in fs::read_dir(self.config.directory.join("leases"))? {
            let path = entry?.path();
            if path.extension().is_none_or(|extension| extension != "json") {
                continue;
            }
            if fs::symlink_metadata(&path)?.file_type().is_symlink() {
                continue;
            }
            let mut file = match OpenOptions::new().read(true).write(true).open(&path) {
                Ok(file) => file,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            match file.try_lock() {
                Ok(()) => {
                    let mut bytes = Vec::new();
                    (&mut file).take(4097).read_to_end(&mut bytes)?;
                    if bytes.len() <= 4096
                        && let Ok(lease) = serde_json::from_slice::<Lease>(&bytes)
                    {
                        state.last_activity_at = state.last_activity_at.max(lease.activity_at);
                        if lease.kind == Usage::Cli {
                            state.last_cli_activity_at =
                                state.last_cli_activity_at.max(lease.activity_at);
                            if state.window.as_ref().is_some_and(|window| {
                                lease.generation < window.generation
                                    && window.opened_at.saturating_sub(lease.activity_at)
                                        < self.config.idle_secs
                            }) {
                                state.window = None;
                            }
                        }
                    }
                    drop(file);
                    let _ = fs::remove_file(path.with_extension("meta"));
                    let _ = fs::remove_file(path);
                }
                Err(std::fs::TryLockError::WouldBlock) => {
                    // Windows locks may prevent reading another handle's bytes.
                    // A read-only companion provides immutable generation metadata.
                    let metadata = fs::read(path.with_extension("meta"));
                    let lease = metadata
                        .ok()
                        .and_then(|bytes| serde_json::from_slice::<Lease>(&bytes).ok())
                        .unwrap_or(Lease {
                            process_id: 0,
                            kind: Usage::Mcp,
                            generation: 0,
                            activity_at: timestamp,
                        });
                    live.push(lease);
                    state.last_activity_at = state.last_activity_at.max(timestamp);
                }
                Err(std::fs::TryLockError::Error(error)) => return Err(error.into()),
            }
        }
        Ok(live)
    }

    fn discard_candidate(&self, prepared: &delivery::Prepared) {
        if prepared.path.parent() == Some(self.config.directory.as_path())
            && prepared
                .path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("candidate-"))
            && fs::symlink_metadata(&prepared.path)
                .is_ok_and(|metadata| metadata.file_type().is_file())
        {
            let _ = fs::remove_file(&prepared.path);
        }
    }

    fn prune_obsolete_candidate(&self, state: &mut State) {
        let current = semver::Version::parse(&self.config.delivery.current_version);
        let obsolete = state.pending.as_ref().is_some_and(|pending| {
            semver::Version::parse(&pending.prepared.version)
                .ok()
                .zip(current.as_ref().ok())
                .is_some_and(|(pending, current)| pending <= *current)
        });
        if obsolete && let Some(pending) = state.pending.take() {
            self.discard_candidate(&pending.prepared);
            state.last_install_attempt_at = None;
        }
    }

    fn cleanup_backups(&self, state: &mut State) {
        state.retained_backups.retain(|path| {
            if path.parent() != self.config.executable.parent()
                || !path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with(".xnip-old-"))
            {
                return false;
            }
            // Windows refuses to delete mapped images; keep those for a later worker.
            fs::remove_file(path).is_err_and(|error| error.kind() != std::io::ErrorKind::NotFound)
        });
    }

    fn poll_delay(&self, state: &State, timestamp: u64) -> Duration {
        let mut seconds = self.config.poll_secs.max(1);
        let deadline = state.pending.as_ref().map(|pending| {
            pending
                .first_seen_at
                .saturating_add(self.config.max_defer_secs)
        });
        let retry = state
            .last_install_attempt_at
            .map(|last| last.saturating_add(FAILURE_RETRY_SECS));
        for event in [Some(state.next_check_at), deadline, retry]
            .into_iter()
            .flatten()
        {
            if event > timestamp {
                seconds = seconds.min(event - timestamp);
            }
        }
        Duration::from_secs(seconds)
    }

    fn remember_error(&self, phase: &str, error: &anyhow::Error) {
        if let Ok(Some(_gate)) = self.gate() {
            let mut state = self.read_state().unwrap_or_default();
            state.phase = phase.into();
            if phase == "worker_failed" {
                state.worker_pid = None;
            }
            state.last_error = Some(format!("{error:#}").chars().take(512).collect());
            let _ = self.save(&state);
        }
    }

    fn spawn(&self, handoff: bool) -> Result<()> {
        #[cfg(windows)]
        {
            use winsafe::co;
            let executable = self
                .config
                .executable
                .to_str()
                .context("non-Unicode executable path")?;
            let arguments = format!(
                "\"{executable}\" {WORKER_COMMAND}{}",
                if handoff { " --handoff" } else { "" }
            );
            let flags = co::CREATE::NO_WINDOW | co::CREATE::NEW_PROCESS_GROUP;
            let launch = |flags| {
                winsafe::CreateProcess(
                    Some(executable),
                    Some(&arguments),
                    None,
                    None,
                    false,
                    flags,
                    &[],
                    None,
                    &mut winsafe::STARTUPINFO::default(),
                )
            };
            // bInheritHandles=false is essential: inheriting an agent's anonymous
            // output pipe would keep its tool call waiting after the foreground exits.
            if launch(flags | co::CREATE::BREAKAWAY_FROM_JOB).is_err() {
                let _process = launch(flags)?;
            }
            Ok(())
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            let mut command = Command::new(&self.config.executable);
            command.arg(WORKER_COMMAND);
            if handoff {
                command.arg("--handoff");
            }
            let mut child = command
                .process_group(0)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
            let _ = std::thread::Builder::new()
                .name("xnip-update-reaper".into())
                .spawn(move || {
                    let _ = child.wait();
                });
            Ok(())
        }
    }

    pub fn run_worker(&self) -> Result<()> {
        self.run_worker_with_handoff(false)
    }

    fn run_worker_with_handoff(&self, handoff: bool) -> Result<()> {
        self.initialize()?;
        let wait_until =
            std::time::Instant::now() + Duration::from_secs(if handoff { 10 } else { 0 });
        let singleton = loop {
            if let Some(guard) = lock(&self.config.directory.join("worker.lock"))? {
                break guard;
            }
            if std::time::Instant::now() >= wait_until {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        loop {
            let timestamp = now();
            let Some(gate) = self.gate()? else {
                std::thread::sleep(Duration::from_millis(100));
                continue;
            };
            let mut state = self.read_state()?;
            state.worker_pid = Some(std::process::id());
            let live = self.sweep(&mut state, timestamp)?;
            if state.window.as_ref().is_some_and(|window| {
                live.iter()
                    .any(|lease| lease.generation < window.generation)
            }) {
                // A startup during an existing session is not a future idle-start permit.
                state.window = None;
            }
            self.cleanup_backups(&mut state);
            self.prune_obsolete_candidate(&mut state);
            let reason = state.install_reason(&live, timestamp, self.config.max_defer_secs);
            let retry_due = state
                .last_install_attempt_at
                .is_none_or(|last| timestamp.saturating_sub(last) >= FAILURE_RETRY_SECS);
            // Install an already-verified eligible candidate before doing fresh network IO.
            if timestamp >= state.next_check_at && (reason.is_none() || !retry_due) {
                state.phase = "checking".into();
                state.last_check_at = Some(timestamp);
                state.next_check_at = timestamp.saturating_add(FAILURE_RETRY_SECS);
                self.save(&state)?;
                drop(gate);
                let mut trace = delivery::DeliveryTrace::default();
                let prepared = delivery::prepare_traced(
                    &self.config.delivery,
                    &self.config.directory,
                    &mut trace,
                );
                let gate = loop {
                    if let Some(gate) = self.gate()? {
                        break gate;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                };
                state = self.read_state()?;
                state.last_delivery = Some(trace);
                match prepared {
                    Ok(Some(prepared)) => {
                        let first_seen_at = state
                            .pending
                            .as_ref()
                            .map_or(timestamp, |pending| pending.first_seen_at);
                        if let Some(previous) = state.pending.replace(Pending {
                            prepared,
                            first_seen_at,
                        }) {
                            self.discard_candidate(&previous.prepared);
                        }
                        state.phase = "prepared".into();
                        state.last_error = None;
                        state.next_check_at = now().saturating_add(self.config.check_interval_secs);
                    }
                    Ok(None) => {
                        state.phase = "current".into();
                        state.last_error = None;
                        state.next_check_at = now().saturating_add(self.config.check_interval_secs);
                    }
                    Err(error) => {
                        state.phase = "check_failed".into();
                        state.last_error = Some(format!("{error:#}").chars().take(512).collect());
                    }
                }
                self.save(&state)?;
                drop(gate);
                continue;
            }
            if let (Some(reason), Some(pending)) = (reason, state.pending.clone())
                && retry_due
            {
                state.last_install_attempt_at = Some(timestamp);
                state.phase = "installing".into();
                self.save(&state)?;
                match delivery::install(&pending.prepared) {
                    Ok(backup) => {
                        state.phase = "installed".into();
                        state.installed_at = Some(now());
                        state.installed_version = Some(pending.prepared.version.clone());
                        state.install_reason = Some(reason);
                        state.last_error = None;
                        state.pending = None;
                        state.window = None;
                        state.worker_pid = None;
                        if let Some(backup) = backup {
                            state.retained_backups.push(backup);
                        }
                        self.discard_candidate(&pending.prepared);
                        self.save(&state)?;
                        // A still-connected MCP must keep receiving future checks, using
                        // the newly installed image after this singleton owner exits.
                        if live.iter().any(|lease| lease.kind == Usage::Mcp)
                            && let Err(error) = self.spawn(true)
                        {
                            state.last_error = Some(format!("monitor handoff: {error}"));
                            self.save(&state)?;
                        }
                        drop(singleton);
                        return Ok(());
                    }
                    Err(error) => {
                        state.next_check_at = state
                            .next_check_at
                            .min(timestamp.saturating_add(FAILURE_RETRY_SECS));
                        state.phase = "install_failed".into();
                        state.last_error = Some(format!("{error:#}").chars().take(512).collect());
                    }
                }
            }
            let idle = live.is_empty()
                && timestamp.saturating_sub(state.last_activity_at) >= self.config.idle_secs;
            let no_monitor_needed =
                state.pending.is_none() && live.iter().all(|lease| lease.kind == Usage::Cli);
            if idle || no_monitor_needed {
                state.worker_pid = None;
                if state.pending.is_some() {
                    state.phase = "waiting_for_next_use".into();
                    state.window = None;
                }
                self.save(&state)?;
                drop(singleton);
                return Ok(());
            }
            let delay = self.poll_delay(&state, timestamp);
            self.save(&state)?;
            drop(gate);
            std::thread::sleep(delay);
        }
    }
}

pub struct Session {
    file: File,
    lease: Lease,
    path: PathBuf,
    start_worker: bool,
    coordinator: Coordinator,
}

impl Drop for Session {
    fn drop(&mut self) {
        self.lease.activity_at = now();
        let _ = self.file.seek(SeekFrom::Start(0));
        let _ = self.file.set_len(0);
        let _ = serde_json::to_writer(&mut self.file, &self.lease);
        if let Ok(Some(_gate)) = self.coordinator.gate()
            && let Ok(mut state) = self.coordinator.read_state()
        {
            state.last_activity_at = state.last_activity_at.max(self.lease.activity_at);
            if self.lease.kind == Usage::Cli {
                state.last_cli_activity_at = state.last_cli_activity_at.max(self.lease.activity_at);
            }
            let _ = self.coordinator.save(&state);
        }
        // The worker also consumes this timestamp if a busy gate skipped the save.
        let _ = fs::remove_file(self.path.with_extension("meta"));
    }
}

/// Called after argument parsing, so discovery/version/invalid commands do no updater work.
pub fn begin_usage(mcp: bool) -> Option<Session> {
    let config = Config::from_env().ok()??;
    let coordinator = Coordinator::new(config);
    let session = match coordinator.begin(if mcp { Usage::Mcp } else { Usage::Cli }, now()) {
        Ok(session) => session,
        Err(error) => {
            coordinator.remember_error("registration_failed", &error);
            return None;
        }
    };
    if session.start_worker
        && let Err(error) = coordinator.spawn(session.lease.kind == Usage::Mcp)
    {
        coordinator.remember_error("spawn_failed", &error);
    }
    Some(session)
}

/// Hidden workers never enter the CLI and never report update success or failure.
pub fn worker_entry() {
    let Ok(Some(config)) = Config::from_env() else {
        return;
    };
    let coordinator = Coordinator::new(config);
    let handoff = std::env::args_os()
        .nth(2)
        .is_some_and(|value| value == "--handoff");
    if let Err(error) = coordinator.run_worker_with_handoff(handoff) {
        coordinator.remember_error("worker_failed", &error);
    }
}

#[cfg(test)]
mod contract_tests;
