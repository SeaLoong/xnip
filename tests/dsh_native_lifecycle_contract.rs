//! Opt-in failure recovery through the official DSH CLI and installed plugin.
//! Unlike the local transaction fixture, these checks execute the real host.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail, ensure};
use xnip::native::{
    self, Capabilities, CheckStatus, CommandLifecycle, Lifecycle, Observation, Phase, Verification,
};

struct FailAfter {
    host: CommandLifecycle,
    phase: Phase,
    fired: bool,
}

impl FailAfter {
    fn checkpoint(&mut self, phase: Phase) -> Result<()> {
        if !self.fired && phase == self.phase {
            self.fired = true;
            bail!("injected failure after real DSH {phase:?}");
        }
        Ok(())
    }
}

impl Lifecycle for FailAfter {
    fn preflight(&mut self) -> Result<Capabilities> {
        let result = self.host.preflight()?;
        self.checkpoint(Phase::Preflight)?;
        Ok(result)
    }

    fn observe(&mut self) -> Result<Observation> {
        self.host.observe()
    }

    fn stage(&mut self, directory: &Path) -> Result<()> {
        self.host.stage(directory)?;
        self.checkpoint(Phase::Stage)
    }

    fn apply(&mut self, directory: &Path) -> Result<()> {
        self.host.apply(directory)?;
        self.checkpoint(Phase::Apply)
    }

    fn activate(&mut self, installed: bool) -> Result<()> {
        self.host.activate(installed)?;
        if installed {
            self.checkpoint(Phase::Activate)?;
        }
        Ok(())
    }

    fn remove(&mut self) -> Result<()> {
        self.host.remove()
    }

    fn verify(&mut self, installed: bool) -> Result<Verification> {
        let result = self.host.verify(installed)?;
        if installed {
            self.checkpoint(Phase::Verify)?;
        }
        Ok(result)
    }
}

fn optional_file(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

#[test]
#[ignore = "requires prepared official DSH and real Node child-process execution"]
fn real_dsh_failures_after_each_install_phase_restore_original_native_editing() -> Result<()> {
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let host = std::env::var_os("XNIP_DSH_NATIVE_HOST").map_or_else(
        || repository.join("target/native-hosts/dsh-0.2.1-rc.5"),
        PathBuf::from,
    );
    let dsh = fs::canonicalize(host.join("node_modules/.bin/dsh"))
        .context("prepare the official DSH host before running this opt-in test")?;
    let node =
        std::env::var_os("XNIP_DSH_NODE").map_or_else(|| PathBuf::from("node"), PathBuf::from);
    let executable = std::env::var_os("XNIP_TEST_BINARY")
        .map_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_xnip")), PathBuf::from);

    for phase in [
        Phase::Preflight,
        Phase::Stage,
        Phase::Apply,
        Phase::Activate,
        Phase::Verify,
    ] {
        let temporary = tempfile::tempdir()?;
        let home = fs::canonicalize(temporary.path())?;
        let initialized = Command::new(&dsh)
            .args(["--profile", "web", "--dump-config"])
            .env("DSH_HOME", &home)
            .env("XNIP_AUTO_UPDATE", "0")
            .output()?;
        ensure!(
            initialized.status.success(),
            "real host initialization failed: {}",
            String::from_utf8_lossy(&initialized.stderr)
        );
        ensure!(
            String::from_utf8_lossy(&initialized.stdout).contains("id: fs-sandbox"),
            "host initialization did not execute its real profile composer"
        );
        let profile = home.join("profiles/web");
        let package_before = fs::read(profile.join("package.json"))?;
        let patch_before = fs::read(profile.join("cordis.patch.yml"))?;
        let lock_before = optional_file(&profile.join("pnpm-lock.yaml"))?;
        ensure!(
            !profile.join("node_modules").exists(),
            "fresh official profile unexpectedly contains dependencies"
        );
        let mut command = CommandLifecycle {
            node: node.clone(),
            driver: repository.join("integrations/dsh-native/lifecycle.mjs"),
            profile: profile.clone(),
            dsh: dsh.clone(),
            plugin: repository.join("integrations/dsh-native"),
            executable: executable.clone(),
        };
        let baseline = command.observe()?;
        let state = home.join("native-installation-state");
        let plan = command.install_plan(state.clone())?;
        let mut failing = FailAfter {
            host: command,
            phase,
            fired: false,
        };
        let error = native::install(&plan, &mut failing)
            .expect_err("real phase failure must abort installation");
        ensure!(
            failing.fired,
            "real {phase:?} failed before reaching the fault injection: {error:#}"
        );
        ensure!(
            format!("{error:#}").contains("injected failure after real DSH"),
            "unexpected installation error: {error:#}"
        );
        ensure!(
            fs::read(profile.join("package.json"))? == package_before,
            "package bytes differ after {phase:?}"
        );
        ensure!(
            fs::read(profile.join("cordis.patch.yml"))? == patch_before,
            "user patch bytes differ after {phase:?}"
        );
        ensure!(
            optional_file(&profile.join("pnpm-lock.yaml"))? == lock_before,
            "lock state differs after {phase:?}"
        );
        ensure!(
            !profile.join("node_modules").exists(),
            "created dependencies remain after {phase:?}"
        );
        ensure!(
            failing.host.observe()? == baseline,
            "catalog or prompt differs after {phase:?}"
        );
        let restored = failing.host.verify(false)?;
        ensure!(
            restored.functional_edit,
            "original native edit did not execute after {phase:?}"
        );
        ensure!(
            !restored.checks.is_empty()
                && restored
                    .checks
                    .iter()
                    .all(|check| check.status == CheckStatus::Passed),
            "original host verification incomplete after {phase:?}"
        );
        if state.join("install-manifest.json").exists() {
            ensure!(
                native::doctor(&state)?.phase == Phase::Restored,
                "manifest does not confirm restoration after {phase:?}: {error:#}"
            );
        }
        eprintln!(
            "real DSH {phase:?}: injected after real phase; original bytes, catalog/prompt and native edit restored"
        );
    }
    Ok(())
}
