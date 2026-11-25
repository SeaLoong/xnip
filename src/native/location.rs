//! Path encoding for retained native state. Snapshot contents, symlink targets
//! and their fingerprints are data and never undergo path substitution.
use super::{
    Lock, Manifest, NativeReport, Phase, profile_lock, read_manifest, report, save,
    validate_manifest,
};
use anyhow::{Context, Result, ensure};
use std::fs;
use std::path::{Component, Path, PathBuf};

const HOME_PATCH: &str = "../../cordis.patch.yml";

impl Manifest {
    fn map_paths(&mut self, map: impl Fn(&Path) -> Result<PathBuf>) -> Result<()> {
        for path in &mut self.preserved_data {
            *path = map(path)?;
        }
        for snapshot in [&mut self.baseline, &mut self.observed]
            .into_iter()
            .chain(self.installed.iter_mut())
            .chain(self.removed.iter_mut())
        {
            snapshot.map_paths(&map)?;
        }
        Ok(())
    }

    fn make_relative(&mut self) -> Result<()> {
        let profile = self.profile.clone();
        let home_patch = profile
            .parent()
            .and_then(Path::parent)
            .map(|home| home.join("cordis.patch.yml"));
        self.map_paths(|path| {
            ensure!(path.is_absolute(), "expected an absolute installation path");
            if let Ok(relative) = path.strip_prefix(&profile) {
                Ok(relative.to_owned())
            } else if Some(path) == home_patch.as_deref() {
                Ok(PathBuf::from(HOME_PATCH))
            } else {
                Ok(path.to_owned())
            }
        })
    }

    fn bind_paths(&mut self, profile: PathBuf) -> Result<()> {
        self.map_paths(|path| {
            if path == Path::new(HOME_PATCH) {
                let home = profile
                    .parent()
                    .and_then(Path::parent)
                    .context("missing profile home")?;
                return Ok(home.join("cordis.patch.yml"));
            }
            ensure!(
                !path.components().any(|part| part == Component::ParentDir),
                "invalid installation path"
            );
            Ok(profile.join(path))
        })?;
        self.profile = profile;
        Ok(())
    }

    pub(super) fn for_storage(&self, state: &Path) -> Result<Self> {
        let mut stored = self.clone();
        stored.make_relative()?;
        if let Ok(suffix) = state.strip_prefix(&self.profile) {
            ensure!(!suffix.as_os_str().is_empty(), "state overlaps profile");
            stored.profile = suffix.components().map(|_| "..").collect();
        }
        // External state has no stable relative relationship to the profile.
        stored.schema_version = 2;
        Ok(stored)
    }

    pub(super) fn resolve_location(&mut self, state: &Path) -> Result<()> {
        match self.schema_version {
            1 => ensure!(self.profile.is_absolute(), "invalid legacy profile path"),
            2 => {
                let profile = if self.profile.is_absolute() {
                    self.profile.clone()
                } else {
                    let parts = self.profile.components();
                    ensure!(
                        parts.clone().count() > 0
                            && parts.clone().all(|p| p == Component::ParentDir),
                        "invalid stored profile location"
                    );
                    state
                        .ancestors()
                        .nth(parts.count())
                        .context("invalid stored profile depth")?
                        .to_owned()
                };
                ensure!(
                    profile.parent().is_some(),
                    "profile cannot be a filesystem root"
                );
                self.bind_paths(profile)?;
            }
            _ => anyhow::bail!("unsupported native manifest schema"),
        }
        Ok(())
    }
}

/// Rebind legacy or externally stored installation state to an explicitly
/// selected copy. Only an exact, complete managed tree can claim the state.
pub fn rebind(state: &Path, profile: &Path) -> Result<NativeReport> {
    let profile = fs::canonicalize(profile)?;
    ensure!(
        profile.is_dir() && profile.parent().is_some(),
        "expected a profile directory"
    );
    read_manifest(state)?;
    let _state_lock = Lock::acquire(state)?;
    let _profile_lock = profile_lock(&profile)?;
    let mut manifest = read_manifest(state)?;
    manifest.resolve_location(state)?;
    ensure!(
        manifest.profile == profile || !state.starts_with(&manifest.profile),
        "copy the state to the destination profile before rebinding it"
    );
    manifest.make_relative()?;
    manifest.bind_paths(profile)?;
    validate_manifest(&manifest, state)?;
    let expected = match manifest.phase {
        Phase::Active => manifest
            .installed
            .as_ref()
            .context("missing installed snapshot")?,
        Phase::Restored => &manifest.baseline,
        _ => anyhow::bail!("finish recovery before rebinding an interrupted installation"),
    };
    ensure!(
        expected.differences(state)?.is_empty(),
        "selected profile differs from the retained managed files"
    );
    manifest.baseline.verify_blobs(state)?;
    expected.verify_blobs(state)?;
    save(state, &manifest)?;
    report(state, &manifest)
}
