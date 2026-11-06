//! Transaction engine contracts using a local filesystem lifecycle fixture.
//! These do not claim that DSH's UI, approval or rewind integration was exercised.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail, ensure};
use tempfile::TempDir;
use xnip::native::{
    self, Capabilities, Check, CheckStatus, InstallPlan, Lifecycle, Observation, Phase,
    Verification,
};

const ORIGINAL_PACKAGE: &[u8] = b"{\r\n  \"name\": \"user-profile\"\r\n}\r\n";
const INSTALLED_PACKAGE: &[u8] = b"{\"name\":\"user-profile\",\"native\":true}\n";

// Independent fault switches exercise each host boundary without coupling the
// installation under test to a fake host state machine.
#[allow(clippy::struct_excessive_bools)]
struct LocalLifecycle {
    profile: PathBuf,
    bundle: PathBuf,
    manage_lock: bool,
    version: String,
    installed_check: CheckStatus,
    checks_empty: bool,
    edit_ok: bool,
    fail_after_apply: bool,
    fail_after_remove: bool,
    apply_calls: usize,
    remove_calls: usize,
    interrupt_at: Option<String>,
}

impl LocalLifecycle {
    fn new(profile: &Path) -> Self {
        Self {
            profile: profile.to_path_buf(),
            bundle: profile.join("node_modules/xnip-native"),
            manage_lock: true,
            version: "0.2.1-rc.5".into(),
            installed_check: CheckStatus::Passed,
            checks_empty: false,
            edit_ok: true,
            fail_after_apply: false,
            fail_after_remove: false,
            apply_calls: 0,
            remove_calls: 0,
            interrupt_at: None,
        }
    }
}

impl Lifecycle for LocalLifecycle {
    fn preflight(&mut self) -> Result<Capabilities> {
        Ok(Capabilities {
            host: "local-fixture".into(),
            version: self.version.clone(),
            level: "fixture".into(),
            mechanism: "filesystem-contract-fixture".into(),
            supported: true,
            gaps: Vec::new(),
        })
    }

    fn observe(&mut self) -> Result<Observation> {
        Ok(Observation {
            catalog: "read/edit/write".into(),
            prompt: fs::read_to_string(self.profile.join("instructions.txt"))?,
        })
    }

    fn stage(&mut self, directory: &Path) -> Result<()> {
        fs::write(
            directory.join("provider.mjs"),
            b"export const backend = 'xnip';\n",
        )?;
        Ok(())
    }

    fn apply(&mut self, directory: &Path) -> Result<()> {
        self.apply_calls += 1;
        fs::write(self.profile.join("package.json"), INSTALLED_PACKAGE)?;
        if self.manage_lock {
            fs::write(self.profile.join("pnpm-lock.yaml"), b"native: enabled\n")?;
        }
        fs::create_dir_all(&self.bundle)?;
        fs::copy(
            directory.join("provider.mjs"),
            self.bundle.join("index.mjs"),
        )?;
        if self.fail_after_apply {
            bail!("injected failure after partial host installation");
        }
        Ok(())
    }

    fn activate(&mut self, installed: bool) -> Result<()> {
        ensure!(
            self.bundle.join("index.mjs").is_file() == installed,
            "provider presence disagrees with activation state"
        );
        if !installed && self.interrupt_at.as_deref() == Some("restore") {
            std::process::exit(73);
        }
        Ok(())
    }

    fn remove(&mut self) -> Result<()> {
        self.remove_calls += 1;
        fs::remove_dir_all(&self.bundle)?;
        if self.interrupt_at.as_deref() == Some("remove") {
            std::process::exit(73);
        }
        fs::write(self.profile.join("package.json"), b"{}\n")?;
        if self.fail_after_remove {
            bail!("injected failure after partial host removal");
        }
        Ok(())
    }

    fn verify(&mut self, installed: bool) -> Result<Verification> {
        ensure!(
            self.bundle.join("index.mjs").is_file() == installed,
            "wrong provider state"
        );
        // A real temporary file edit proves the fixture's functional check runs;
        // the host adapter's own live tests must prove its editing dispatch.
        let fixture = tempfile::tempdir()?;
        let path = fixture.path().join("config.env");
        fs::write(&path, b"PORT=8080\r\nKEEP=yes\r\n")?;
        let source = fs::read(&path)?;
        let after = String::from_utf8(source)?.replace("8080", "3000");
        fs::write(&path, after.as_bytes())?;
        ensure!(
            fs::read(path)? == b"PORT=3000\r\nKEEP=yes\r\n",
            "fixture edit bytes"
        );
        let checks = if installed && self.checks_empty {
            Vec::new()
        } else {
            vec![Check {
                name: "fixture provider state and temporary edit".into(),
                status: if installed {
                    self.installed_check.clone()
                } else {
                    CheckStatus::Passed
                },
                detail: "Local lifecycle transaction test, not DSH readiness evidence".into(),
            }]
        };
        Ok(Verification {
            checks,
            functional_edit: !installed || self.edit_ok,
        })
    }
}

struct Fixture {
    _directory: TempDir,
    plan: InstallPlan,
    lifecycle: LocalLifecycle,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(directory.path()).unwrap();
        let profile = root.join("profile");
        fs::create_dir_all(profile.join("node_modules/existing")).unwrap();
        fs::write(profile.join("package.json"), ORIGINAL_PACKAGE).unwrap();
        fs::write(profile.join("pnpm-lock.yaml"), b"lockfileVersion: '9.0'\n").unwrap();
        fs::write(
            profile.join("node_modules/existing/index.js"),
            b"module.exports = 42;\n",
        )
        .unwrap();
        fs::write(
            profile.join("instructions.txt"),
            b"Prefer read/edit/write\n",
        )
        .unwrap();
        fs::write(profile.join("cordis.patch.yml"), b"# user configuration\n").unwrap();
        let plan = InstallPlan {
            host: "local-fixture".into(),
            host_version: "0.2.1-rc.5".into(),
            profile: profile.clone(),
            state_dir: root.join("installation-state"),
            managed_paths: ["package.json", "pnpm-lock.yaml", "node_modules"]
                .map(|name| profile.join(name))
                .into(),
            observed_paths: vec![profile.join("cordis.patch.yml")],
            preserved_data: vec![profile.join("journal")],
        };
        let lifecycle = LocalLifecycle::new(&profile);
        Self {
            _directory: directory,
            plan,
            lifecycle,
        }
    }

    fn assert_original(&self) {
        assert_eq!(
            fs::read(self.plan.profile.join("package.json")).unwrap(),
            ORIGINAL_PACKAGE
        );
        assert_eq!(
            fs::read(self.plan.profile.join("pnpm-lock.yaml")).unwrap(),
            b"lockfileVersion: '9.0'\n"
        );
        assert_eq!(
            fs::read(self.plan.profile.join("node_modules/existing/index.js")).unwrap(),
            b"module.exports = 42;\n"
        );
        assert!(!self.plan.profile.join("node_modules/xnip-native").exists());
        assert_eq!(
            fs::read(self.plan.profile.join("cordis.patch.yml")).unwrap(),
            b"# user configuration\n"
        );
    }
}

#[test]
fn install_uninstall_restores_original_bytes_and_retains_evidence_until_explicit_clean() {
    let mut fixture = Fixture::new();
    let result = native::install(&fixture.plan, &mut fixture.lifecycle).unwrap();
    assert_eq!(result.phase, Phase::Active);
    assert_eq!(result.managed_differences, Vec::<PathBuf>::new());
    assert_eq!(
        fs::read(fixture.plan.profile.join("package.json")).unwrap(),
        INSTALLED_PACKAGE
    );
    let result = native::uninstall(&fixture.plan.state_dir, &mut fixture.lifecycle).unwrap();
    assert_eq!(result.phase, Phase::Restored);
    assert_eq!(result.managed_differences, Vec::<PathBuf>::new());
    fixture.assert_original();
    native::verify(&fixture.plan.state_dir, &mut fixture.lifecycle, true).unwrap();
    assert!(
        fixture
            .plan
            .state_dir
            .join("install-manifest.json")
            .is_file()
    );
    assert!(native::clean(&fixture.plan.state_dir, false).is_err());
    native::clean(&fixture.plan.state_dir, true).unwrap();
    assert!(
        !fixture
            .plan
            .state_dir
            .join("install-manifest.json")
            .exists()
    );
    fixture.assert_original();
}

#[test]
fn each_install_phase_failure_leaves_original_host_usable() {
    for failure in [
        Phase::Preflight,
        Phase::Stage,
        Phase::Apply,
        Phase::Activate,
        Phase::Verify,
    ] {
        let mut fixture = Fixture::new();
        let error =
            native::install_with_hook(&fixture.plan, &mut fixture.lifecycle, &mut |phase| {
                if phase == failure {
                    bail!("injected phase failure: {phase:?}");
                }
                Ok(())
            })
            .unwrap_err();
        assert!(format!("{error:#}").contains("injected phase failure"));
        fixture.assert_original();
        if failure != Phase::Preflight {
            assert_eq!(
                native::doctor(&fixture.plan.state_dir).unwrap().phase,
                Phase::Restored
            );
        }
    }
}

#[test]
fn partial_host_apply_is_rolled_back_with_created_directories_removed() {
    let mut fixture = Fixture::new();
    fixture.lifecycle.fail_after_apply = true;
    let error = native::install(&fixture.plan, &mut fixture.lifecycle).unwrap_err();
    assert!(format!("{error:#}").contains("partial host installation"));
    fixture.assert_original();
    assert_eq!(
        native::doctor(&fixture.plan.state_dir).unwrap().phase,
        Phase::Restored
    );
}

#[test]
fn unverified_skipped_failed_or_empty_checks_cannot_commit_an_installation() {
    for status in [
        CheckStatus::Unverified,
        CheckStatus::Skipped,
        CheckStatus::Failed,
    ] {
        let mut fixture = Fixture::new();
        fixture.lifecycle.installed_check = status;
        assert!(native::install(&fixture.plan, &mut fixture.lifecycle).is_err());
        fixture.assert_original();
        assert_eq!(
            native::doctor(&fixture.plan.state_dir).unwrap().phase,
            Phase::Restored
        );
    }
    for empty in [true, false] {
        let mut fixture = Fixture::new();
        fixture.lifecycle.checks_empty = empty;
        fixture.lifecycle.edit_ok = empty;
        assert!(native::install(&fixture.plan, &mut fixture.lifecycle).is_err());
        fixture.assert_original();
    }
}

#[test]
fn uninstall_refuses_subsequent_user_changes_without_running_host_removal() {
    let mut fixture = Fixture::new();
    native::install(&fixture.plan, &mut fixture.lifecycle).unwrap();
    let user_edit = b"{\"native\":true,\"userAdded\":true}\n";
    fs::write(fixture.plan.profile.join("package.json"), user_edit).unwrap();
    let error = native::uninstall(&fixture.plan.state_dir, &mut fixture.lifecycle).unwrap_err();
    assert!(error.to_string().contains("profile changed"));
    assert_eq!(fixture.lifecycle.remove_calls, 0);
    assert_eq!(
        fs::read(fixture.plan.profile.join("package.json")).unwrap(),
        user_edit
    );
    assert!(
        fixture
            .plan
            .profile
            .join("node_modules/xnip-native/index.mjs")
            .is_file()
    );
    assert_eq!(
        native::doctor(&fixture.plan.state_dir).unwrap().phase,
        Phase::Active
    );
}

#[test]
fn host_version_drift_requires_revalidation_before_uninstall() {
    let mut fixture = Fixture::new();
    native::install(&fixture.plan, &mut fixture.lifecycle).unwrap();
    fixture.lifecycle.version = "0.2.2".into();
    let error = native::uninstall(&fixture.plan.state_dir, &mut fixture.lifecycle).unwrap_err();
    assert!(error.to_string().contains("version changed"));
    assert_eq!(fixture.lifecycle.remove_calls, 0);
    assert_eq!(
        fs::read(fixture.plan.profile.join("package.json")).unwrap(),
        INSTALLED_PACKAGE
    );
}

#[test]
fn corrupted_backup_is_rejected_before_host_removal_or_profile_mutation() {
    use sha2::{Digest, Sha256};

    let mut fixture = Fixture::new();
    native::install(&fixture.plan, &mut fixture.lifecycle).unwrap();
    let digest = format!("{:x}", Sha256::digest(ORIGINAL_PACKAGE));
    fs::write(
        fixture.plan.state_dir.join("blobs").join(digest),
        b"corrupted backup",
    )
    .unwrap();
    let error = native::uninstall(&fixture.plan.state_dir, &mut fixture.lifecycle).unwrap_err();
    assert!(format!("{error:#}").contains("integrity"));
    assert_eq!(fixture.lifecycle.remove_calls, 0);
    assert_eq!(
        fs::read(fixture.plan.profile.join("package.json")).unwrap(),
        INSTALLED_PACKAGE
    );
    assert!(
        fixture
            .plan
            .profile
            .join("node_modules/xnip-native/index.mjs")
            .is_file()
    );
}

fn private_bundle_fixture() -> Fixture {
    let mut fixture = Fixture::new();
    let bundle = fixture.plan.profile.join(".xnip-native-package");
    fixture.plan.managed_paths = vec![fixture.plan.profile.join("package.json"), bundle.clone()];
    fixture
        .plan
        .observed_paths
        .push(fixture.plan.profile.join("pnpm-lock.yaml"));
    fixture.lifecycle.bundle = bundle;
    fixture.lifecycle.manage_lock = false;
    fixture
}

fn directory_state(root: &Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, path: &Path, entries: &mut std::collections::BTreeMap<PathBuf, Vec<u8>>) {
        let metadata = fs::symlink_metadata(path).unwrap();
        let mut state = Vec::new();
        if metadata.file_type().is_symlink() {
            state.push(b'L');
            state.extend_from_slice(fs::read_link(path).unwrap().as_os_str().as_encoded_bytes());
        } else if metadata.is_dir() {
            state.push(b'D');
        } else {
            state.push(b'F');
            state.extend(fs::read(path).unwrap());
        }
        entries.insert(path.strip_prefix(root).unwrap().to_path_buf(), state);
        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            for entry in fs::read_dir(path).unwrap() {
                visit(root, &entry.unwrap().path(), entries);
            }
        }
    }
    let mut entries = std::collections::BTreeMap::new();
    visit(root, root, &mut entries);
    entries
}

#[test]
fn profile_bundle_keeps_existing_dependency_tree_and_lock_outside_snapshots() {
    let mut fixture = private_bundle_fixture();
    let dependencies = fixture.plan.profile.join("node_modules");
    fs::write(
        dependencies.join("large-existing.bin"),
        vec![73; 2 * 1024 * 1024],
    )
    .unwrap();
    let before = directory_state(&dependencies);
    let lock = fs::read(fixture.plan.profile.join("pnpm-lock.yaml")).unwrap();
    native::install(&fixture.plan, &mut fixture.lifecycle).unwrap();
    assert!(fixture.lifecycle.bundle.join("index.mjs").is_file());
    assert_eq!(directory_state(&dependencies), before);
    let blob_bytes: u64 = fs::read_dir(fixture.plan.state_dir.join("blobs"))
        .unwrap()
        .map(|entry| entry.unwrap().metadata().unwrap().len())
        .sum();
    assert!(
        blob_bytes < 65536,
        "dependency contents entered the snapshots"
    );
    native::uninstall(&fixture.plan.state_dir, &mut fixture.lifecycle).unwrap();
    fixture.assert_original();
    assert_eq!(directory_state(&dependencies), before);
    assert_eq!(
        fs::read(fixture.plan.profile.join("pnpm-lock.yaml")).unwrap(),
        lock
    );
    assert!(!fixture.lifecycle.bundle.exists());
}

#[cfg(unix)]
fn shared_bundle_fixture(relative: bool) -> (Fixture, PathBuf, PathBuf) {
    use std::os::unix::fs::symlink;
    let mut fixture = private_bundle_fixture();
    let root = fixture.plan.profile.parent().unwrap();
    let shared = root.join("shared-dependencies");
    let dependencies = fixture.plan.profile.join("node_modules");
    fs::rename(&dependencies, &shared).unwrap();
    let target = if relative {
        PathBuf::from("../shared-dependencies")
    } else {
        shared.clone()
    };
    symlink(target, &dependencies).unwrap();
    fixture.plan.observed_paths.push(dependencies);
    let neighbor = root.join("neighbor-profile");
    fs::create_dir(&neighbor).unwrap();
    fs::write(neighbor.join("package.json"), b"{\"name\":\"neighbor\"}\n").unwrap();
    symlink("../shared-dependencies", neighbor.join("node_modules")).unwrap();
    (fixture, shared, neighbor)
}

#[cfg(unix)]
#[test]
fn profile_bundle_preserves_shared_links_and_neighbor_profile_through_install_and_uninstall() {
    for relative in [false, true] {
        let (mut fixture, shared, neighbor) = shared_bundle_fixture(relative);
        let before = directory_state(&shared);
        let neighbor_before = directory_state(&neighbor);
        let dependencies = fixture.plan.profile.join("node_modules");
        let link = fs::read_link(&dependencies).unwrap();
        let lock = fs::read(fixture.plan.profile.join("pnpm-lock.yaml")).unwrap();
        native::install(&fixture.plan, &mut fixture.lifecycle).unwrap();
        assert_eq!(directory_state(&shared), before);
        assert_eq!(directory_state(&neighbor), neighbor_before);
        assert_eq!(fs::read_link(&dependencies).unwrap(), link);
        native::uninstall(&fixture.plan.state_dir, &mut fixture.lifecycle).unwrap();
        fixture.assert_original();
        assert_eq!(directory_state(&shared), before);
        assert_eq!(directory_state(&neighbor), neighbor_before);
        assert_eq!(fs::read_link(dependencies).unwrap(), link);
        assert_eq!(
            fs::read(fixture.plan.profile.join("pnpm-lock.yaml")).unwrap(),
            lock
        );
        assert!(!fixture.lifecycle.bundle.exists());
    }
}

#[cfg(unix)]
#[test]
fn profile_bundle_failure_phases_restore_only_profile_owned_paths() {
    for phase in [Phase::Stage, Phase::Apply, Phase::Activate, Phase::Verify] {
        let (mut fixture, shared, neighbor) = shared_bundle_fixture(true);
        let before = directory_state(&shared);
        let neighbor_before = directory_state(&neighbor);
        let result =
            native::install_with_hook(&fixture.plan, &mut fixture.lifecycle, &mut |current| {
                if current == phase {
                    bail!("injected profile-bundle phase failure");
                }
                Ok(())
            });
        assert!(result.is_err());
        fixture.assert_original();
        assert_eq!(directory_state(&shared), before);
        assert_eq!(directory_state(&neighbor), neighbor_before);
        assert!(!fixture.lifecycle.bundle.exists());
    }
    let (mut fixture, shared, neighbor) = shared_bundle_fixture(false);
    let before = directory_state(&shared);
    let neighbor_before = directory_state(&neighbor);
    fixture.lifecycle.fail_after_apply = true;
    assert!(native::install(&fixture.plan, &mut fixture.lifecycle).is_err());
    fixture.assert_original();
    assert_eq!(directory_state(&shared), before);
    assert_eq!(directory_state(&neighbor), neighbor_before);
    assert!(!fixture.lifecycle.bundle.exists());
}

#[cfg(unix)]
#[test]
fn profile_bundle_preserves_a_user_changed_dependency_link() {
    use std::os::unix::fs::symlink;
    let (mut fixture, shared, neighbor) = shared_bundle_fixture(true);
    let before = directory_state(&shared);
    let neighbor_before = directory_state(&neighbor);
    native::install(&fixture.plan, &mut fixture.lifecycle).unwrap();
    let replacement = fixture
        .plan
        .profile
        .parent()
        .unwrap()
        .join("new-dependencies");
    fs::create_dir(&replacement).unwrap();
    fs::write(replacement.join("user.txt"), b"new dependencies\n").unwrap();
    let dependencies = fixture.plan.profile.join("node_modules");
    fs::remove_file(&dependencies).unwrap();
    symlink(&replacement, &dependencies).unwrap();
    assert!(native::uninstall(&fixture.plan.state_dir, &mut fixture.lifecycle).is_err());
    assert_eq!(fixture.lifecycle.remove_calls, 0);
    assert_eq!(fs::read_link(dependencies).unwrap(), replacement);
    assert_eq!(directory_state(&shared), before);
    assert_eq!(directory_state(&neighbor), neighbor_before);
}

#[cfg(unix)]
#[test]
fn managed_dependency_root_links_keep_the_original_snapshot_boundary() {
    let (mut fixture, shared, _) = shared_bundle_fixture(true);
    fixture
        .plan
        .managed_paths
        .push(fixture.plan.profile.join("node_modules"));
    let before = directory_state(&shared);
    let error = native::install(&fixture.plan, &mut fixture.lifecycle).unwrap_err();
    assert!(format!("{error:#}").contains("symlink"));
    assert_eq!(fixture.lifecycle.apply_calls, 0);
    assert_eq!(directory_state(&shared), before);
}

#[cfg(unix)]
#[test]
fn original_modes_and_dependency_symlink_are_restored_exactly() {
    use std::os::unix::fs::{PermissionsExt, symlink};

    let mut fixture = Fixture::new();
    let dependency = fixture.plan.profile.join("node_modules/existing/index.js");
    fs::set_permissions(&dependency, fs::Permissions::from_mode(0o750)).unwrap();
    let link = fixture.plan.profile.join("node_modules/existing-link");
    symlink("existing", &link).unwrap();
    native::install(&fixture.plan, &mut fixture.lifecycle).unwrap();
    native::uninstall(&fixture.plan.state_dir, &mut fixture.lifecycle).unwrap();
    assert_eq!(
        fs::metadata(&dependency).unwrap().permissions().mode() & 0o777,
        0o750
    );
    assert_eq!(fs::read_link(&link).unwrap(), Path::new("existing"));
    fixture.assert_original();
    assert_eq!(
        fs::metadata(&fixture.plan.state_dir)
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(fixture.plan.state_dir.join("install-manifest.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}

#[test]
fn duplicate_managed_roots_are_rejected_before_lifecycle_mutation() {
    let mut fixture = Fixture::new();
    fixture
        .plan
        .managed_paths
        .push(fixture.plan.profile.join("node_modules/existing"));
    assert!(native::install(&fixture.plan, &mut fixture.lifecycle).is_err());
    fixture.assert_original();
}

#[test]
fn profile_lock_prevents_parallel_operations_even_with_different_state_directories() {
    let mut fixture = Fixture::new();
    let mut other_plan = fixture.plan.clone();
    other_plan.state_dir = fixture
        .plan
        .state_dir
        .with_file_name("other-installation-state");
    let mut other_lifecycle = LocalLifecycle::new(&fixture.plan.profile);
    let mut lock_tested = false;
    native::install_with_hook(&fixture.plan, &mut fixture.lifecycle, &mut |phase| {
        if phase == Phase::Stage {
            let error = native::install(&other_plan, &mut other_lifecycle).unwrap_err();
            ensure!(
                format!("{error:#}").contains("owns this profile"),
                "wrong lock failure: {error:#}"
            );
            ensure!(
                !other_plan.state_dir.join("install-manifest.json").exists(),
                "concurrent operation created manifest"
            );
            lock_tested = true;
        }
        Ok(())
    })
    .unwrap();
    assert!(lock_tested);
    native::uninstall(&fixture.plan.state_dir, &mut fixture.lifecycle).unwrap();
    fixture.assert_original();
}

#[test]
fn interrupted_activation_can_recover_from_the_durable_manifest() {
    let mut fixture = Fixture::new();
    native::install(&fixture.plan, &mut fixture.lifecycle).unwrap();
    let manifest_path = fixture.plan.state_dir.join("install-manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    // Reproduce the on-disk checkpoint preceding activation after host mutation.
    manifest["phase"] = "activate".into();
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    let report = native::recover(&fixture.plan.state_dir, &mut fixture.lifecycle).unwrap();
    assert_eq!(report.phase, Phase::Restored);
    fixture.assert_original();
    native::verify(&fixture.plan.state_dir, &mut fixture.lifecycle, true).unwrap();
}

#[test]
fn interrupted_uninstall_child() {
    let Some(profile) = std::env::var_os("XNIP_TEST_INTERRUPTED_PROFILE") else {
        return;
    };
    let state = PathBuf::from(std::env::var_os("XNIP_TEST_INTERRUPTED_STATE").unwrap());
    let mut lifecycle = LocalLifecycle::new(Path::new(&profile));
    lifecycle.interrupt_at = Some(std::env::var("XNIP_TEST_INTERRUPT_AT").unwrap());
    native::uninstall(&state, &mut lifecycle).unwrap();
    panic!("uninstall should exit at its selected interruption boundary");
}

fn interrupt_uninstall(fixture: &Fixture, boundary: &str) {
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "interrupted_uninstall_child", "--nocapture"])
        .env("XNIP_TEST_INTERRUPTED_PROFILE", &fixture.plan.profile)
        .env("XNIP_TEST_INTERRUPTED_STATE", &fixture.plan.state_dir)
        .env("XNIP_TEST_INTERRUPT_AT", boundary)
        .output()
        .unwrap();
    assert_eq!(
        result.status.code(),
        Some(73),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        native::doctor(&fixture.plan.state_dir).unwrap().phase,
        Phase::Removing
    );
}

#[test]
fn interrupted_removal_and_restored_activation_resume_from_retained_trees() {
    for boundary in ["remove", "restore"] {
        let mut fixture = Fixture::new();
        native::install(&fixture.plan, &mut fixture.lifecycle).unwrap();
        interrupt_uninstall(&fixture, boundary);
        let report = native::recover(&fixture.plan.state_dir, &mut fixture.lifecycle).unwrap();
        assert_eq!(report.phase, Phase::Restored);
        fixture.assert_original();
        native::verify(&fixture.plan.state_dir, &mut fixture.lifecycle, true).unwrap();
    }
}

#[test]
fn recovery_preserves_user_bytes_added_after_interrupted_removal() {
    let mut fixture = Fixture::new();
    native::install(&fixture.plan, &mut fixture.lifecycle).unwrap();
    interrupt_uninstall(&fixture, "remove");
    let package = fixture.plan.profile.join("package.json");
    let user_content = b"{\"userEditedAfterInterruption\":true}\r\n";
    fs::write(&package, user_content).unwrap();
    let error = native::recover(&fixture.plan.state_dir, &mut fixture.lifecycle).unwrap_err();
    assert!(format!("{error:#}").contains("retained restoration states"));
    assert_eq!(fs::read(package).unwrap(), user_content);
    assert!(
        fixture
            .plan
            .state_dir
            .join("install-manifest.json")
            .is_file()
    );
}

#[test]
fn cli_rejects_state_for_another_profile_before_lifecycle_commands() {
    let mut first = Fixture::new();
    let mut second = Fixture::new();
    native::install(&first.plan, &mut first.lifecycle).unwrap();
    native::install(&second.plan, &mut second.lifecycle).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let driver = directory.path().join("lifecycle.mjs");
    fs::write(&driver, b"throw new Error('lifecycle must not run');\n").unwrap();
    let first_manifest = fs::read(first.plan.state_dir.join("install-manifest.json")).unwrap();
    let second_manifest = fs::read(second.plan.state_dir.join("install-manifest.json")).unwrap();
    for verb in ["uninstall", "recover", "verify"] {
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_xnip"))
            .args(["native", verb, "--profile"])
            .arg(&second.plan.profile)
            .arg("--state-dir")
            .arg(&first.plan.state_dir)
            .arg("--plugin")
            .arg(directory.path())
            .arg("--dsh")
            .arg(std::env::current_exe().unwrap())
            .env("XNIP_AUTO_UPDATE", "0")
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("belongs to another profile"),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    for fixture in [&first, &second] {
        assert_eq!(
            fs::read(fixture.plan.profile.join("package.json")).unwrap(),
            INSTALLED_PACKAGE
        );
        assert!(
            fixture
                .plan
                .profile
                .join("node_modules/xnip-native/index.mjs")
                .is_file()
        );
    }
    assert_eq!(
        fs::read(first.plan.state_dir.join("install-manifest.json")).unwrap(),
        first_manifest
    );
    assert_eq!(
        fs::read(second.plan.state_dir.join("install-manifest.json")).unwrap(),
        second_manifest
    );
}

#[test]
fn active_or_already_restored_installation_is_not_an_interrupted_operation() {
    let mut fixture = Fixture::new();
    native::install(&fixture.plan, &mut fixture.lifecycle).unwrap();
    assert!(native::recover(&fixture.plan.state_dir, &mut fixture.lifecycle).is_err());
    assert_eq!(
        fs::read(fixture.plan.profile.join("package.json")).unwrap(),
        INSTALLED_PACKAGE
    );
    native::uninstall(&fixture.plan.state_dir, &mut fixture.lifecycle).unwrap();
    assert!(native::recover(&fixture.plan.state_dir, &mut fixture.lifecycle).is_err());
    fixture.assert_original();
}

#[test]
fn observed_user_configuration_drift_prevents_uninstall_without_overwriting_it() {
    let mut fixture = Fixture::new();
    native::install(&fixture.plan, &mut fixture.lifecycle).unwrap();
    let user_patch = b"# changed by user while native was installed\n";
    fs::write(fixture.plan.profile.join("cordis.patch.yml"), user_patch).unwrap();
    assert!(native::uninstall(&fixture.plan.state_dir, &mut fixture.lifecycle).is_err());
    assert_eq!(fixture.lifecycle.remove_calls, 0);
    assert_eq!(
        fs::read(fixture.plan.profile.join("cordis.patch.yml")).unwrap(),
        user_patch
    );
    assert_eq!(
        fs::read(fixture.plan.profile.join("package.json")).unwrap(),
        INSTALLED_PACKAGE
    );
}

#[test]
fn partial_uninstall_failure_restores_the_previous_installed_host() {
    let mut fixture = Fixture::new();
    native::install(&fixture.plan, &mut fixture.lifecycle).unwrap();
    fixture.lifecycle.fail_after_remove = true;
    let error = native::uninstall(&fixture.plan.state_dir, &mut fixture.lifecycle).unwrap_err();
    assert!(format!("{error:#}").contains("partial host removal"));
    assert_eq!(
        fs::read(fixture.plan.profile.join("package.json")).unwrap(),
        INSTALLED_PACKAGE
    );
    assert!(
        fixture
            .plan
            .profile
            .join("node_modules/xnip-native/index.mjs")
            .is_file()
    );
    assert_eq!(
        native::doctor(&fixture.plan.state_dir).unwrap().phase,
        Phase::Active
    );
    native::verify(&fixture.plan.state_dir, &mut fixture.lifecycle, false).unwrap();
    fixture.lifecycle.fail_after_remove = false;
    native::uninstall(&fixture.plan.state_dir, &mut fixture.lifecycle).unwrap();
    fixture.assert_original();
}

#[test]
fn stage_time_user_changes_are_preserved_when_installation_fails() {
    let mut fixture = Fixture::new();
    let package = fixture.plan.profile.join("package.json");
    let user_content = b"{\"userChangedBeforeApply\":true}\n";
    let error = native::install_with_hook(&fixture.plan, &mut fixture.lifecycle, &mut |phase| {
        if phase == Phase::Stage {
            fs::write(&package, user_content)?;
            bail!("injected stage failure after concurrent user edit");
        }
        Ok(())
    })
    .unwrap_err();
    let detail = format!("{error:#}");
    assert!(detail.contains("refusing to overwrite user changes"));
    assert_eq!(fs::read(&package).unwrap(), user_content);
    assert!(
        !fixture
            .plan
            .profile
            .join("node_modules/xnip-native")
            .exists()
    );
    assert_eq!(fixture.lifecycle.remove_calls, 0);
    let report = native::doctor(&fixture.plan.state_dir).unwrap();
    assert_eq!(report.phase, Phase::RecoveryRequired);
    assert!(report.managed_differences.contains(&package));
    assert!(
        fixture
            .plan
            .state_dir
            .join("install-manifest.json")
            .is_file()
    );
    // Retrying recovery must preserve the same ownership boundary: it cannot
    // replace an edit made before the installer applied any host changes.
    assert!(native::recover(&fixture.plan.state_dir, &mut fixture.lifecycle).is_err());
    assert_eq!(fs::read(&package).unwrap(), user_content);
}

#[test]
fn apply_phase_user_changes_are_not_owned_until_preapply_checks_complete() {
    for hook_fails in [false, true] {
        let mut fixture = Fixture::new();
        let package = fixture.plan.profile.join("package.json");
        let user_content = b"{\"userChangedAtApplyBoundary\":true}\r\n";
        let error =
            native::install_with_hook(&fixture.plan, &mut fixture.lifecycle, &mut |phase| {
                if phase == Phase::Apply {
                    fs::write(&package, user_content)?;
                    if hook_fails {
                        bail!("apply hook failed after user edit");
                    }
                }
                Ok(())
            })
            .unwrap_err();
        let detail = format!("{error:#}");
        assert!(
            detail.contains("refusing to overwrite user changes"),
            "{detail}"
        );
        assert_eq!(fs::read(&package).unwrap(), user_content);
        assert_eq!(fixture.lifecycle.apply_calls, 0);
        assert!(
            !fixture
                .plan
                .profile
                .join("node_modules/xnip-native")
                .exists()
        );
        assert_eq!(
            fs::read(fixture.plan.profile.join("pnpm-lock.yaml")).unwrap(),
            b"lockfileVersion: '9.0'\n"
        );
        assert_eq!(
            native::doctor(&fixture.plan.state_dir).unwrap().phase,
            Phase::RecoveryRequired
        );
        // Recovery reloads the durable ownership marker instead of inferring
        // write ownership from the recorded APPLY/RecoveryRequired phase.
        for _ in 0..2 {
            assert!(native::recover(&fixture.plan.state_dir, &mut fixture.lifecycle).is_err());
            assert_eq!(fs::read(&package).unwrap(), user_content);
            assert_eq!(fixture.lifecycle.apply_calls, 0);
        }
    }
}

#[cfg(unix)]
#[test]
fn installer_state_symlink_is_rejected_without_touching_the_link_target() {
    use std::os::unix::fs::symlink;

    let mut fixture = Fixture::new();
    let outside = fixture
        .plan
        .state_dir
        .with_file_name("unrelated-user-directory");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("important.txt"), b"keep this data\n").unwrap();
    symlink(&outside, &fixture.plan.state_dir).unwrap();
    let error = native::install(&fixture.plan, &mut fixture.lifecycle).unwrap_err();
    assert!(format!("{error:#}").contains("symlink"));
    assert_eq!(
        fs::read(outside.join("important.txt")).unwrap(),
        b"keep this data\n"
    );
    assert!(!outside.join("install-manifest.json").exists());
    assert!(!outside.join("operation.lock").exists());
    fixture.assert_original();
}

#[cfg(unix)]
#[test]
fn successful_installation_keeps_profile_backup_contents_private() {
    use std::os::unix::fs::PermissionsExt;

    let mut fixture = Fixture::new();
    native::install(&fixture.plan, &mut fixture.lifecycle).unwrap();
    assert_eq!(
        fs::metadata(&fixture.plan.state_dir)
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(fixture.plan.state_dir.join("install-manifest.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    let blobs = fs::read_dir(fixture.plan.state_dir.join("blobs"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert!(
        blobs.iter().any(|blob| blob.is_file()),
        "no backup contents were written"
    );
    for blob in blobs {
        assert_eq!(
            fs::metadata(blob).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn empty_state_diagnostics_and_recovery_preserve_the_observed_directory() {
    for (name, code, serialized_code) in [
        (
            "missing",
            native::NativeStateCode::StateMissing,
            "state_missing",
        ),
        ("empty", native::NativeStateCode::StateEmpty, "state_empty"),
        (
            "lock-only",
            native::NativeStateCode::StateEmpty,
            "state_empty",
        ),
    ] {
        let mut fixture = Fixture::new();
        let state = &fixture.plan.state_dir;
        if name != "missing" {
            fs::create_dir(state).unwrap();
        }
        if name == "lock-only" {
            fs::write(state.join("operation.lock"), b"stable coordination inode\n").unwrap();
        }
        #[cfg(unix)]
        let mode_before = {
            use std::os::unix::fs::PermissionsExt;
            fs::metadata(state)
                .ok()
                .map(|metadata| metadata.permissions().mode())
        };
        for error in [
            native::doctor(state).unwrap_err(),
            native::recover(state, &mut fixture.lifecycle).unwrap_err(),
        ] {
            let diagnostic = error.downcast_ref::<native::NativeStateError>().unwrap();
            assert_eq!(diagnostic.code, code);
            assert_eq!(&diagnostic.state_dir, state);
            let json = serde_json::to_value(diagnostic).unwrap();
            assert_eq!(json["code"], serialized_code);
            assert_eq!(json["state_dir"].as_str(), state.to_str());
            assert!(json["message"].as_str().unwrap().contains("--state-dir"));
            let message = diagnostic.to_string();
            assert!(message.contains(&code.to_string()));
            assert!(message.contains(&state.display().to_string()));
        }
        match name {
            "missing" => assert!(!state.exists()),
            "empty" => assert_eq!(fs::read_dir(state).unwrap().count(), 0),
            "lock-only" => {
                assert_eq!(fs::read_dir(state).unwrap().count(), 1);
                assert_eq!(
                    fs::read(state.join("operation.lock")).unwrap(),
                    b"stable coordination inode\n"
                );
            }
            _ => unreachable!(),
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(state)
                    .ok()
                    .map(|metadata| metadata.permissions().mode()),
                mode_before
            );
        }
        assert_eq!(fixture.lifecycle.apply_calls, 0);
        assert_eq!(fixture.lifecycle.remove_calls, 0);
        fixture.assert_original();
    }
}

#[test]
fn missing_manifest_preserves_retained_evidence_during_diagnostics_and_recovery() {
    let mut fixture = Fixture::new();
    let state = &fixture.plan.state_dir;
    fs::create_dir_all(state.join("blobs")).unwrap();
    let evidence = state.join("blobs/retained-backup");
    fs::write(&evidence, b"retained original bytes\r\n").unwrap();
    for error in [
        native::doctor(state).unwrap_err(),
        native::recover(state, &mut fixture.lifecycle).unwrap_err(),
    ] {
        let diagnostic = error.downcast_ref::<native::NativeStateError>().unwrap();
        assert_eq!(diagnostic.code, native::NativeStateCode::ManifestMissing);
        assert!(diagnostic.message.contains("Preserve this directory"));
    }
    assert_eq!(fs::read(&evidence).unwrap(), b"retained original bytes\r\n");
    assert_eq!(fs::read_dir(state).unwrap().count(), 1);
    assert!(!state.join("operation.lock").exists());
    let error = native::clean(state, true).unwrap_err();
    assert_eq!(
        error
            .downcast_ref::<native::NativeStateError>()
            .unwrap()
            .code,
        native::NativeStateCode::ManifestMissing
    );
    assert_eq!(fs::read(evidence).unwrap(), b"retained original bytes\r\n");
    fixture.assert_original();
}

#[test]
fn clean_missing_state_is_idempotent_and_preserves_its_absence() {
    let fixture = Fixture::new();
    for _ in 0..2 {
        assert_eq!(
            native::clean(&fixture.plan.state_dir, true).unwrap(),
            Vec::<PathBuf>::new()
        );
        assert!(!fixture.plan.state_dir.exists());
    }
    assert!(native::clean(&fixture.plan.state_dir, false).is_err());
    fixture.assert_original();
}

#[test]
fn clean_retains_the_same_coordination_inode_for_reentry_and_later_installation() {
    let mut fixture = Fixture::new();
    native::install(&fixture.plan, &mut fixture.lifecycle).unwrap();
    native::uninstall(&fixture.plan.state_dir, &mut fixture.lifecycle).unwrap();
    let lock_path = fixture.plan.state_dir.join("operation.lock");
    let opened_before_clean = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&lock_path)
        .unwrap();
    let removed = native::clean(&fixture.plan.state_dir, true).unwrap();
    assert!(removed.contains(&fixture.plan.state_dir.join("install-manifest.json")));
    let entries: Vec<_> = fs::read_dir(&fixture.plan.state_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(entries, vec![lock_path.clone()]);
    let error = native::doctor(&fixture.plan.state_dir).unwrap_err();
    assert_eq!(
        error
            .downcast_ref::<native::NativeStateError>()
            .unwrap()
            .code,
        native::NativeStateCode::StateEmpty
    );
    // A handle opened before purge must still exclude a newly opened operation.
    opened_before_clean.try_lock().unwrap();
    let error = native::clean(&fixture.plan.state_dir, true).unwrap_err();
    assert!(format!("{error:#}").contains("owns this state directory"));
    let error = native::install(&fixture.plan, &mut fixture.lifecycle).unwrap_err();
    assert!(format!("{error:#}").contains("owns this state directory"));
    opened_before_clean.unlock().unwrap();
    assert_eq!(
        native::clean(&fixture.plan.state_dir, true).unwrap(),
        Vec::<PathBuf>::new()
    );
    assert_eq!(fs::read_dir(&fixture.plan.state_dir).unwrap().count(), 1);
    assert_eq!(
        native::install(&fixture.plan, &mut fixture.lifecycle)
            .unwrap()
            .phase,
        Phase::Active
    );
}
