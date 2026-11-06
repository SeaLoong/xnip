//! Reversible host installation with durable snapshots and honest verification.
//!
//! Host commands only modify declared managed paths. The installer keeps the
//! original and installed trees so removal refuses to overwrite subsequent user
//! changes. Audit logs and editing journals are reported separately.

pub(crate) mod operations;
mod snapshot;

use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use snapshot::{Snapshot, atomic_json};

const MANIFEST: &str = "install-manifest.json";
const COORDINATION_LOCK: &str = "operation.lock";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capabilities {
    pub host: String,
    pub version: String,
    pub level: String,
    pub mechanism: String,
    pub supported: bool,
    #[serde(default)]
    pub gaps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Observation {
    pub catalog: String,
    pub prompt: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Passed,
    Failed,
    Unverified,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Check {
    pub name: String,
    pub status: CheckStatus,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Verification {
    pub checks: Vec<Check>,
    pub functional_edit: bool,
}

impl Verification {
    fn require_pass(&self) -> Result<()> {
        ensure!(
            self.functional_edit,
            "host verification did not perform a successful real edit"
        );
        ensure!(
            !self.checks.is_empty(),
            "host verification executed no checks"
        );
        let incomplete: Vec<_> = self
            .checks
            .iter()
            .filter(|check| check.status != CheckStatus::Passed)
            .map(|check| format!("{}: {:?}: {}", check.name, check.status, check.detail))
            .collect();
        ensure!(
            incomplete.is_empty(),
            "host verification incomplete: {}",
            incomplete.join("; ")
        );
        Ok(())
    }
}

/// All paths are absolute. Managed roots must be disjoint and below profile.
#[derive(Debug, Clone)]
pub struct InstallPlan {
    pub host: String,
    pub host_version: String,
    pub profile: PathBuf,
    pub state_dir: PathBuf,
    pub managed_paths: Vec<PathBuf>,
    /// Read-only user configuration dependencies: detect drift, never restore them.
    pub observed_paths: Vec<PathBuf>,
    pub preserved_data: Vec<PathBuf>,
}

/// A host must implement real activation and edit checks, not just inspect a PID.
pub trait Lifecycle {
    fn preflight(&mut self) -> Result<Capabilities>;
    fn observe(&mut self) -> Result<Observation>;
    fn stage(&mut self, directory: &Path) -> Result<()>;
    fn apply(&mut self, directory: &Path) -> Result<()>;
    fn activate(&mut self, installed: bool) -> Result<()>;
    fn remove(&mut self) -> Result<()>;
    fn verify(&mut self, installed: bool) -> Result<Verification>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Preflight,
    Snapshot,
    Stage,
    Apply,
    Activate,
    Verify,
    Active,
    Removing,
    Restored,
    RecoveryRequired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub schema_version: u32,
    pub host: String,
    pub host_version: String,
    pub xnip_version: String,
    pub profile: PathBuf,
    pub phase: Phase,
    /// Preserve the failure boundary even when phase becomes `RecoveryRequired`;
    /// pre-apply failures never own concurrent changes made by the user.
    #[serde(default)]
    pub failed_phase: Option<Phase>,
    /// Durable ownership boundary: a phase label alone does not mean the host
    /// mutator ran. Set only after the final pre-apply drift checks succeed.
    #[serde(default)]
    pub apply_started: bool,
    pub capabilities: Capabilities,
    pub baseline_observation: Observation,
    pub installed_observation: Option<Observation>,
    pub preserved_data: Vec<PathBuf>,
    pub verification: Option<Verification>,
    baseline: Snapshot,
    observed: Snapshot,
    installed: Option<Snapshot>,
    /// Fingerprints after official removal, before restoring the original tree.
    #[serde(default)]
    removed: Option<Snapshot>,
}

#[derive(Debug, Serialize)]
pub struct NativeReport {
    pub host: String,
    pub host_version: String,
    pub phase: Phase,
    pub level: String,
    pub managed_differences: Vec<PathBuf>,
    pub preserved_data: Vec<PathBuf>,
    pub gaps: Vec<String>,
    pub verification: Option<Verification>,
}

/// Distinguish an empty installation from retained data needing inspection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeStateCode {
    StateMissing,
    StateEmpty,
    ManifestMissing,
}

impl std::fmt::Display for NativeStateCode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::StateMissing => "state_missing",
            Self::StateEmpty => "state_empty",
            Self::ManifestMissing => "manifest_missing",
        })
    }
}

/// An actionable state diagnostic, retained through `anyhow` for CLI JSON output.
#[derive(Debug, Serialize, thiserror::Error)]
#[error("{code}: {message} (state directory: {state_dir})")]
pub struct NativeStateError {
    pub code: NativeStateCode,
    pub state_dir: PathBuf,
    pub message: String,
}

impl NativeStateError {
    fn new(state: &Path, code: NativeStateCode) -> Self {
        let message = match code {
            NativeStateCode::StateMissing => {
                "Native installation state directory is missing. Select the --state-dir used during installation."
            }
            NativeStateCode::StateEmpty => {
                "Native installation data is empty. Any retained operation.lock coordinates concurrent commands. Recovery requires a retained --state-dir containing install-manifest.json."
            }
            NativeStateCode::ManifestMissing => {
                "Installation data remains, and install-manifest.json is missing. Preserve this directory for inspection and select a --state-dir containing the retained installation manifest."
            }
        };
        Self {
            code,
            state_dir: state.to_path_buf(),
            message: message.into(),
        }
    }
}

/// Cleanup clears installation data while retaining the stable coordination inode.
#[derive(Debug, Serialize)]
pub(crate) struct CleanReport {
    pub state_dir: PathBuf,
    pub installation_data: &'static str,
    pub removed: Vec<PathBuf>,
    pub retained_coordination_paths: Vec<PathBuf>,
}

impl CleanReport {
    fn cleared(state: &Path, removed: Vec<PathBuf>) -> Self {
        let lock = state.join(COORDINATION_LOCK);
        Self {
            state_dir: state.to_path_buf(),
            installation_data: "cleared",
            removed,
            retained_coordination_paths: lock.is_file().then_some(lock).into_iter().collect(),
        }
    }
}

struct Lock(File);

impl Lock {
    fn acquire(state: &Path) -> Result<Self> {
        ensure_private_state(state)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(state.join(COORDINATION_LOCK))?;
        file.try_lock()
            .context("another native installation operation owns this state directory")?;
        Ok(Self(file))
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

fn validate_state_path(state: &Path) -> Result<()> {
    ensure!(state.is_absolute(), "state directory must be absolute");
    ensure!(
        !state.components().any(|part| matches!(
            part,
            std::path::Component::ParentDir | std::path::Component::CurDir
        )),
        "state directory contains traversal components"
    );
    let mut parent = Some(state);
    while let Some(path) = parent {
        if let Ok(metadata) = fs::symlink_metadata(path) {
            ensure!(
                !metadata.file_type().is_symlink(),
                "state directory/ancestor is a symlink: {}",
                path.display()
            );
        }
        parent = path.parent();
    }
    Ok(())
}

/// Resolve CLI paths after checking the supplied path's ownership boundary.
pub(crate) fn resolve_state_path(state: &Path) -> Result<PathBuf> {
    let absolute = if state.is_absolute() {
        state.to_path_buf()
    } else {
        std::env::current_dir()?.join(state)
    };
    validate_state_path(&absolute)?;
    crate::core::transaction::normalize(&absolute)
}

fn ensure_private_state(state: &Path) -> Result<()> {
    validate_state_path(state)?;
    fs::create_dir_all(state)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(state, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn profile_lock(profile: &Path) -> Result<Lock> {
    use sha2::{Digest, Sha256};
    let canonical = fs::canonicalize(profile)?;
    let hash = format!(
        "{:x}",
        Sha256::digest(canonical.as_os_str().as_encoded_bytes())
    );
    let directory = canonical
        .parent()
        .context("profile parent missing")?
        .join(".xnip-native-locks");
    ensure_private_state(&directory)?;
    // Keep the inode stable: removing a lock file while another operation has
    // opened it creates two independent locks. OS locks are released on crashes.
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join(format!("{hash}.lock")))?;
    file.try_lock()
        .context("another native operation owns this profile")?;
    Ok(Lock(file))
}

/// Restore an interrupted operation from its durable baseline. This explicit
/// recovery operation does not infer that a surviving host process was stopped.
pub fn recover(state: &Path, lifecycle: &mut dyn Lifecycle) -> Result<NativeReport> {
    // An empty recovery request leaves the directory exactly as observed.
    load(state)?;
    let _lock = Lock::acquire(state)?;
    let mut manifest = load(state)?;
    let _profile_lock = profile_lock(&manifest.profile)?;
    ensure!(
        !matches!(manifest.phase, Phase::Active | Phase::Restored),
        "no interrupted operation to recover"
    );
    preflight(&manifest.host, &manifest.host_version, lifecycle)?;
    rollback(state, &mut manifest, lifecycle)?;
    report(state, &manifest)
}

fn save(state: &Path, manifest: &Manifest) -> Result<()> {
    atomic_json(&state.join(MANIFEST), manifest)
}

fn canonical_state(state: &Path) -> Result<()> {
    ensure!(state.is_absolute(), "state directory must be absolute");
    match fs::symlink_metadata(state) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(NativeStateError::new(state, NativeStateCode::StateMissing).into());
        }
        Err(error) => return Err(error.into()),
        Ok(_) => {}
    }
    ensure!(
        fs::canonicalize(state)? == state,
        "state directory must be its real canonical path"
    );
    Ok(())
}

fn missing_manifest(state: &Path) -> Result<NativeStateError> {
    for entry in fs::read_dir(state)? {
        let entry = entry?;
        if entry.file_name() != COORDINATION_LOCK || !entry.file_type()?.is_file() {
            return Ok(NativeStateError::new(
                state,
                NativeStateCode::ManifestMissing,
            ));
        }
    }
    Ok(NativeStateError::new(state, NativeStateCode::StateEmpty))
}

fn load(state: &Path) -> Result<Manifest> {
    canonical_state(state)?;
    let bytes = match fs::read(state.join(MANIFEST)) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(missing_manifest(state)?.into());
        }
        result => result?,
    };
    let manifest: Manifest = serde_json::from_slice(&bytes)?;
    ensure!(
        manifest.schema_version == 1,
        "unsupported native manifest schema"
    );
    ensure!(
        fs::canonicalize(&manifest.profile)? == manifest.profile,
        "manifest profile is no longer its canonical path"
    );
    manifest.baseline.validate_paths(&manifest.profile, state)?;
    manifest.observed.validate_observed_paths(state)?;
    if let Some(installed) = &manifest.installed {
        installed.validate_paths(&manifest.profile, state)?;
        ensure!(
            installed.roots == manifest.baseline.roots,
            "manifest snapshot root mismatch"
        );
    }
    if let Some(removed) = &manifest.removed {
        removed.validate_paths(&manifest.profile, state)?;
        ensure!(
            removed.roots == manifest.baseline.roots,
            "manifest snapshot root mismatch"
        );
    }
    Ok(manifest)
}

/// The canonical profile associated with retained installation evidence.
pub fn installation_profile(state: &Path) -> Result<PathBuf> {
    Ok(load(state)?.profile)
}

fn preflight(host: &str, version: &str, lifecycle: &mut dyn Lifecycle) -> Result<Capabilities> {
    let capabilities = lifecycle.preflight()?;
    ensure!(
        capabilities.host == host,
        "host identity changed: {}",
        capabilities.host
    );
    ensure!(
        capabilities.version == version,
        "host version changed: expected {version}, got {}; revalidation required",
        capabilities.version
    );
    ensure!(
        capabilities.supported,
        "native replacement unsupported: {}",
        capabilities.gaps.join("; ")
    );
    Ok(capabilities)
}

/// Install and verify, rolling back every failed phase. The callback permits
/// deterministic failure injection without production environment variables.
pub fn install(plan: &InstallPlan, lifecycle: &mut dyn Lifecycle) -> Result<NativeReport> {
    install_with_hook(plan, lifecycle, &mut |_| Ok(()))
}

pub fn install_with_hook(
    plan: &InstallPlan,
    lifecycle: &mut dyn Lifecycle,
    hook: &mut dyn FnMut(Phase) -> Result<()>,
) -> Result<NativeReport> {
    validate_plan(plan)?;
    let _profile_lock = profile_lock(&plan.profile)?;
    let _lock = Lock::acquire(&plan.state_dir)?;
    ensure!(
        !plan.state_dir.join(MANIFEST).exists(),
        "native manifest already exists; verify or uninstall it first"
    );
    hook(Phase::Preflight)?;
    let capabilities = preflight(&plan.host, &plan.host_version, lifecycle)?;
    let baseline_observation = lifecycle.observe()?;
    let baseline = Snapshot::capture(&plan.managed_paths, &plan.state_dir)?;
    let observed = Snapshot::capture(&plan.observed_paths, &plan.state_dir)?;
    let mut manifest = Manifest {
        schema_version: 1,
        host: plan.host.clone(),
        host_version: plan.host_version.clone(),
        xnip_version: crate::VERSION.into(),
        profile: plan.profile.clone(),
        phase: Phase::Snapshot,
        failed_phase: None,
        apply_started: false,
        capabilities,
        baseline_observation,
        installed_observation: None,
        preserved_data: plan.preserved_data.clone(),
        verification: None,
        baseline,
        observed,
        installed: None,
        removed: None,
    };
    save(&plan.state_dir, &manifest)?;
    let stage = plan.state_dir.join("stage");
    let attempt = (|| -> Result<()> {
        for phase in [Phase::Stage, Phase::Apply, Phase::Activate, Phase::Verify] {
            manifest.phase = phase;
            save(&plan.state_dir, &manifest)?;
            hook(phase)?;
            match phase {
                Phase::Stage => {
                    fs::create_dir_all(&stage)?;
                    lifecycle.stage(&stage)?;
                    ensure!(
                        manifest.baseline.differences(&plan.state_dir)?.is_empty(),
                        "host staging modified managed files"
                    );
                }
                Phase::Apply => {
                    ensure!(
                        manifest.baseline.differences(&plan.state_dir)?.is_empty(),
                        "profile changed before apply"
                    );
                    ensure!(
                        manifest.observed.differences(&plan.state_dir)?.is_empty(),
                        "user configuration changed before apply"
                    );
                    let mut owned = manifest.clone();
                    owned.apply_started = true;
                    save(&plan.state_dir, &owned)?;
                    manifest = owned;
                    lifecycle.apply(&stage)?;
                    manifest.installed =
                        Some(Snapshot::capture(&plan.managed_paths, &plan.state_dir)?);
                    save(&plan.state_dir, &manifest)?;
                }
                Phase::Activate => lifecycle.activate(true)?,
                Phase::Verify => {
                    let verification = lifecycle.verify(true)?;
                    manifest.verification = Some(verification.clone());
                    save(&plan.state_dir, &manifest)?;
                    verification.require_pass()?;
                    ensure!(
                        manifest.observed.differences(&plan.state_dir)?.is_empty(),
                        "host modified read-only user configuration"
                    );
                    manifest.installed_observation = Some(lifecycle.observe()?);
                }
                _ => unreachable!("install phase"),
            }
        }
        manifest.installed = Some(Snapshot::capture(&plan.managed_paths, &plan.state_dir)?);
        manifest.phase = Phase::Active;
        save(&plan.state_dir, &manifest)?;
        remove_stage(&stage)?;
        Ok(())
    })();
    if let Err(error) = attempt {
        manifest.failed_phase = Some(manifest.phase);
        return match rollback(&plan.state_dir, &mut manifest, lifecycle) {
            Ok(()) => Err(error.context("native installation failed; original host restored")),
            Err(rollback_error) => {
                manifest.phase = Phase::RecoveryRequired;
                let _ = save(&plan.state_dir, &manifest);
                Err(error.context(format!("rollback incomplete: {rollback_error:#}; retain {} and restore its baseline snapshots", plan.state_dir.join(MANIFEST).display())))
            }
        };
    }
    report(&plan.state_dir, &manifest)
}

fn rollback(state: &Path, manifest: &mut Manifest, lifecycle: &mut dyn Lifecycle) -> Result<()> {
    // A phase label is saved before hooks and drift validation. Until the
    // separate ownership marker is durable, no host mutation has been started
    // and even an APPLY failure cannot own a concurrent profile change.
    if !manifest.apply_started {
        ensure!(
            manifest.baseline.differences(state)?.is_empty(),
            "profile drift before apply; refusing to overwrite user changes"
        );
    }
    let current = Snapshot::inspect(&manifest.baseline.roots, state)?;
    if let Some(installed) = &manifest.installed {
        let mut previous = vec![installed];
        if let Some(removed) = &manifest.removed {
            previous.push(removed);
        }
        manifest
            .baseline
            .validate_restoration_source(&current, &previous)?;
    }
    manifest.baseline.restore(state, &current)?;
    lifecycle.activate(false)?;
    let verification = lifecycle.verify(false)?;
    verification.require_pass()?;
    ensure!(
        lifecycle.observe()? == manifest.baseline_observation,
        "restored catalog or prompt differs from preflight"
    );
    ensure!(
        manifest.baseline.differences(state)?.is_empty(),
        "rollback left managed path differences"
    );
    manifest.phase = Phase::Restored;
    manifest.verification = Some(verification);
    save(state, manifest)?;
    remove_stage(&state.join("stage"))?;
    Ok(())
}

/// Removal refuses any user modification after installation before calling host.
pub fn uninstall(state: &Path, lifecycle: &mut dyn Lifecycle) -> Result<NativeReport> {
    let _lock = Lock::acquire(state)?;
    let mut manifest = load(state)?;
    let _profile_lock = profile_lock(&manifest.profile)?;
    ensure!(
        manifest.phase == Phase::Active,
        "installation is not active; verify/recover its retained manifest first"
    );
    preflight(&manifest.host, &manifest.host_version, lifecycle)?;
    ensure!(
        manifest.observed.differences(state)?.is_empty(),
        "read-only user configuration changed after installation"
    );
    let installed = manifest
        .installed
        .as_ref()
        .context("missing installed snapshot")?
        .clone();
    let differences = installed.differences(state)?;
    ensure!(
        differences.is_empty(),
        "profile changed after installation; refusing to overwrite: {differences:?}"
    );
    // Validate original backup contents before asking the host lifecycle to
    // remove anything; a corrupt backup must leave the current host untouched.
    manifest.baseline.verify_blobs(state)?;
    installed.verify_blobs(state)?;
    manifest.phase = Phase::Removing;
    save(state, &manifest)?;
    let attempt = (|| -> Result<()> {
        lifecycle.remove()?;
        let current = Snapshot::inspect(&manifest.baseline.roots, state)?;
        manifest.removed = Some(current.clone());
        save(state, &manifest)?;
        manifest.baseline.restore(state, &current)?;
        lifecycle.activate(false)?;
        let verification = lifecycle.verify(false)?;
        verification.require_pass()?;
        ensure!(
            lifecycle.observe()? == manifest.baseline_observation,
            "restored catalog or prompt differs from preflight"
        );
        ensure!(
            manifest.baseline.differences(state)?.is_empty(),
            "managed files did not return to baseline"
        );
        manifest.verification = Some(verification);
        manifest.phase = Phase::Restored;
        save(state, &manifest)?;
        Ok(())
    })();
    if let Err(error) = attempt {
        manifest.failed_phase = Some(manifest.phase);
        if error.downcast_ref::<snapshot::RestoreConflict>().is_some() {
            manifest.phase = Phase::RecoveryRequired;
            let _ = save(state, &manifest);
            return Err(error.context(format!(
                "concurrent profile change preserved; recovery evidence retained at {}",
                state.join(MANIFEST).display()
            )));
        }
        let rollback = (|| -> Result<()> {
            let current = Snapshot::inspect(&installed.roots, state)?;
            installed.restore(state, &current)?;
            lifecycle.activate(true)?;
            lifecycle.verify(true)?.require_pass()?;
            manifest.phase = Phase::Active;
            save(state, &manifest)
        })();
        if let Err(rollback_error) = rollback {
            manifest.phase = Phase::RecoveryRequired;
            let _ = save(state, &manifest);
            return Err(error.context(format!(
                "uninstall rollback incomplete: {rollback_error:#}; retained {}",
                state.join(MANIFEST).display()
            )));
        }
        return Err(error.context("uninstall failed; installed host restored"));
    }
    report(state, &manifest)
}

pub fn verify(state: &Path, lifecycle: &mut dyn Lifecycle, restored: bool) -> Result<NativeReport> {
    let _lock = Lock::acquire(state)?;
    let mut manifest = load(state)?;
    let _profile_lock = profile_lock(&manifest.profile)?;
    ensure!(
        manifest.observed.differences(state)?.is_empty(),
        "read-only user configuration differs from baseline"
    );
    preflight(&manifest.host, &manifest.host_version, lifecycle)?;
    let expected = if restored {
        &manifest.baseline
    } else {
        manifest
            .installed
            .as_ref()
            .context("installation was never committed")?
    };
    let differences = expected.differences(state)?;
    ensure!(
        differences.is_empty(),
        "managed files differ from expected state: {differences:?}"
    );
    let verification = lifecycle.verify(!restored)?;
    verification.require_pass()?;
    let observation = lifecycle.observe()?;
    let expected_observation = if restored {
        &manifest.baseline_observation
    } else {
        manifest
            .installed_observation
            .as_ref()
            .context("missing installed observation")?
    };
    ensure!(
        &observation == expected_observation,
        "host tools or prompt changed"
    );
    manifest.verification = Some(verification);
    if restored {
        manifest.phase = Phase::Restored;
    }
    save(state, &manifest)?;
    report(state, &manifest)
}

/// Read-only status. A retained restored manifest is deliberate recovery evidence.
pub fn doctor(state: &Path) -> Result<NativeReport> {
    let manifest = load(state)?;
    report(state, &manifest)
}

fn report(state: &Path, manifest: &Manifest) -> Result<NativeReport> {
    let snapshot = if manifest.phase == Phase::Restored {
        &manifest.baseline
    } else {
        manifest.installed.as_ref().unwrap_or(&manifest.baseline)
    };
    let mut managed_differences = snapshot.differences(state)?;
    managed_differences.extend(manifest.observed.differences(state)?);
    managed_differences.sort();
    managed_differences.dedup();
    Ok(NativeReport {
        host: manifest.host.clone(),
        host_version: manifest.host_version.clone(),
        phase: manifest.phase,
        level: manifest.capabilities.level.clone(),
        managed_differences,
        preserved_data: manifest
            .preserved_data
            .iter()
            .filter(|path| path.exists())
            .cloned()
            .collect(),
        gaps: manifest.capabilities.gaps.clone(),
        verification: manifest.verification.clone(),
    })
}

/// Clean only this installation's backups after a verified restoration. Editing
/// journals and .bak files are never inferred or deleted from arbitrary paths.
pub fn clean(state: &Path, purge_user_data: bool) -> Result<Vec<PathBuf>> {
    Ok(clean_report(state, purge_user_data)?.removed)
}

pub(crate) fn clean_report(state: &Path, purge_user_data: bool) -> Result<CleanReport> {
    ensure!(
        purge_user_data,
        "clean requires explicit purge_user_data for retained backup contents"
    );
    if let Err(error) = canonical_state(state) {
        if error
            .downcast_ref::<NativeStateError>()
            .is_some_and(|state| state.code == NativeStateCode::StateMissing)
        {
            return Ok(CleanReport::cleared(state, Vec::new()));
        }
        return Err(error);
    }
    let _lock = Lock::acquire(state)?;
    let manifest = match load(state) {
        Err(error)
            if error
                .downcast_ref::<NativeStateError>()
                .is_some_and(|state| state.code == NativeStateCode::StateEmpty) =>
        {
            return Ok(CleanReport::cleared(state, Vec::new()));
        }
        result => result?,
    };
    let _profile_lock = profile_lock(&manifest.profile)?;
    ensure!(
        manifest.phase == Phase::Restored,
        "cannot purge an active or incomplete installation"
    );
    ensure!(
        manifest.baseline.differences(state)?.is_empty(),
        "baseline drift prevents backup purge"
    );
    let mut removed = Vec::new();
    for name in ["blobs", "stage"] {
        let path = state.join(name);
        if path.exists() {
            fs::remove_dir_all(&path)?;
            removed.push(path);
        }
    }
    fs::remove_file(state.join(MANIFEST))?;
    removed.push(state.join(MANIFEST));
    // Keep operation.lock's inode stable for commands that already opened it.
    Ok(CleanReport::cleared(state, removed))
}

fn remove_stage(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_dir_all(path)?;
    }
    Ok(())
}

fn validate_plan(plan: &InstallPlan) -> Result<()> {
    ensure!(
        !plan.host.is_empty() && !plan.host_version.is_empty(),
        "missing host identity/version"
    );
    ensure!(
        plan.profile.is_absolute() && plan.state_dir.is_absolute(),
        "profile/state paths must be absolute"
    );
    ensure!(
        fs::canonicalize(&plan.profile)? == plan.profile,
        "profile must be its real canonical directory"
    );
    ensure!(plan.profile.is_dir(), "profile is not a directory");
    ensure!(!plan.managed_paths.is_empty(), "no managed paths declared");
    Snapshot::validate_roots(&plan.managed_paths, &plan.profile, &plan.state_dir)
}

/// JSON bridge to the packaged Node lifecycle driver; no shell interpretation.
#[derive(Debug, Clone)]
pub struct CommandLifecycle {
    pub node: PathBuf,
    pub driver: PathBuf,
    pub profile: PathBuf,
    pub dsh: PathBuf,
    pub plugin: PathBuf,
    pub executable: PathBuf,
}

impl CommandLifecycle {
    fn call<T: serde::de::DeserializeOwned>(
        &self,
        verb: &str,
        stage: Option<&Path>,
        installed: bool,
    ) -> Result<T> {
        let mut command = Command::new(&self.node);
        command
            .arg(&self.driver)
            .arg(verb)
            .arg("--profile")
            .arg(&self.profile)
            .arg("--dsh")
            .arg(&self.dsh)
            .arg("--plugin")
            .arg(&self.plugin)
            .arg("--executable")
            .arg(&self.executable)
            .env("XNIP_AUTO_UPDATE", "0");
        if let Some(stage) = stage {
            command.arg("--stage").arg(stage);
        }
        if installed {
            command.arg("--installed");
        }
        let output = command
            .output()
            .with_context(|| format!("start host lifecycle {verb}"))?;
        ensure!(
            output.status.success(),
            "host lifecycle {verb} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout)
            .with_context(|| format!("invalid host lifecycle {verb} JSON"))
    }

    pub fn install_plan(&mut self, state_dir: PathBuf) -> Result<InstallPlan> {
        let capabilities = self.preflight()?;
        let profile = fs::canonicalize(&self.profile)?;
        let managed_paths = ["package.json", ".xnip-native-package"]
            .iter()
            .map(|name| profile.join(name))
            .collect();
        let mut observed_paths = vec![
            profile.join("cordis.patch.yml"),
            profile.join("pnpm-lock.yaml"),
        ];
        // Dependency contents belong to the profile's existing package manager.
        // A root link is an opaque observation; ordinary dependency trees stay
        // outside both installation snapshots and filesystem mutations.
        let dependencies = profile.join("node_modules");
        if fs::symlink_metadata(&dependencies)
            .is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            observed_paths.push(dependencies);
        }
        if let Some(home) = profile.parent().and_then(Path::parent) {
            observed_paths.push(home.join("cordis.patch.yml"));
        }
        let preserved_data = [".plugin-manager/logs", ".dsh-market"]
            .iter()
            .map(|name| profile.join(name))
            .collect();
        Ok(InstallPlan {
            host: capabilities.host,
            host_version: capabilities.version,
            profile,
            state_dir,
            managed_paths,
            observed_paths,
            preserved_data,
        })
    }
}

impl Lifecycle for CommandLifecycle {
    fn preflight(&mut self) -> Result<Capabilities> {
        self.call("preflight", None, false)
    }
    fn observe(&mut self) -> Result<Observation> {
        self.call("observe", None, false)
    }
    fn stage(&mut self, directory: &Path) -> Result<()> {
        let _: serde_json::Value = self.call("stage", Some(directory), false)?;
        Ok(())
    }
    fn apply(&mut self, directory: &Path) -> Result<()> {
        let _: serde_json::Value = self.call("apply", Some(directory), false)?;
        Ok(())
    }
    fn activate(&mut self, installed: bool) -> Result<()> {
        let _: serde_json::Value = self.call("activate", None, installed)?;
        Ok(())
    }
    fn remove(&mut self) -> Result<()> {
        let _: serde_json::Value = self.call("remove", None, false)?;
        Ok(())
    }
    fn verify(&mut self, installed: bool) -> Result<Verification> {
        self.call("verify", None, installed)
    }
}
