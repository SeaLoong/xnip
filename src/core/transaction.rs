//! Validated file plans, no-clobber moves, durable recovery snapshots and rollback.
use super::atomic;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs::{self, Permissions};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Clone)]
struct Entry {
    before: Option<Vec<u8>>,
    after: Option<Vec<u8>>,
    permissions: Option<Permissions>,
    origin: PathBuf,
}

#[derive(Default)]
pub struct Plan {
    entries: BTreeMap<PathBuf, Entry>,
}

#[derive(Debug, thiserror::Error)]
#[error("commit failed: {message}; rollback: {rollback}; recovery journal: {journal}")]
pub struct CommitError {
    pub message: String,
    pub rollback: String,
    pub journal: PathBuf,
}

#[derive(Debug, Default)]
pub struct Report {
    pub files: Vec<PathBuf>,
    pub diff: String,
    /// Retained on cleanup failure; ordinary successful commits remove the journal.
    pub journal: Option<PathBuf>,
}

/// Resolve existing aliases and the nearest existing ancestor of a new path.
pub fn normalize(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    if fs::symlink_metadata(&absolute).is_ok() {
        return Ok(fs::canonicalize(&absolute)?);
    }
    let mut cursor = absolute.as_path();
    let mut tail = Vec::new();
    loop {
        match fs::metadata(cursor) {
            Ok(metadata) => {
                if !metadata.is_dir() {
                    bail!("parent is not a directory: {}", cursor.display());
                }
                let mut result = fs::canonicalize(cursor)?;
                for name in tail.iter().rev() {
                    result.push(name);
                }
                return Ok(result);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        tail.push(
            cursor
                .file_name()
                .context("cannot resolve missing path components")?
                .to_os_string(),
        );
        cursor = cursor.parent().context("path has no existing ancestor")?;
    }
}

fn snapshot(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                bail!("not a regular file: {}", path.display());
            }
            Ok(Some(
                fs::read(path).with_context(|| format!("read {}", path.display()))?,
            ))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn reject_link(path: &Path) -> Result<()> {
    if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        bail!(
            "file lifecycle operations do not follow symbolic links: {}",
            path.display()
        );
    }
    Ok(())
}

impl Plan {
    fn entry(&mut self, path: &Path) -> Result<(PathBuf, &mut Entry)> {
        let key = normalize(path)?;
        #[cfg(windows)]
        let key = self
            .entries
            .keys()
            .find(|existing| {
                existing
                    .to_string_lossy()
                    .eq_ignore_ascii_case(&key.to_string_lossy())
            })
            .cloned()
            .unwrap_or(key);
        if !self.entries.contains_key(&key) {
            let before = snapshot(&key)?;
            let permissions = if before.is_some() {
                Some(fs::metadata(&key)?.permissions())
            } else {
                None
            };
            self.entries.insert(
                key.clone(),
                Entry {
                    after: before.clone(),
                    before,
                    permissions,
                    origin: key.clone(),
                },
            );
        }
        Ok((
            key.clone(),
            self.entries.get_mut(&key).expect("entry inserted"),
        ))
    }
    pub fn read(&mut self, path: &Path) -> Result<Vec<u8>> {
        self.entry(path)?
            .1
            .after
            .clone()
            .with_context(|| format!("file does not exist in plan: {}", path.display()))
    }
    pub fn update(&mut self, path: &Path, bytes: Vec<u8>) -> Result<()> {
        let (_, entry) = self.entry(path)?;
        if entry.after.is_none() {
            bail!("update target does not exist: {}", path.display());
        }
        entry.after = Some(bytes);
        Ok(())
    }
    pub fn create(&mut self, path: &Path, bytes: Vec<u8>) -> Result<()> {
        reject_link(path)?;
        let (_, entry) = self.entry(path)?;
        if entry.after.is_some() {
            bail!("create target already exists: {}", path.display());
        }
        entry.after = Some(bytes);
        Ok(())
    }
    pub fn delete(&mut self, path: &Path, expected: Option<&[u8]>) -> Result<()> {
        reject_link(path)?;
        let (_, entry) = self.entry(path)?;
        let bytes = entry
            .after
            .as_deref()
            .context("delete target does not exist")?;
        if expected.is_some_and(|expected| expected != bytes) {
            bail!("delete content precondition failed");
        }
        entry.after = None;
        Ok(())
    }
    pub fn rename(&mut self, from: &Path, to: &Path, expected: Option<&[u8]>) -> Result<()> {
        reject_link(from)?;
        reject_link(to)?;
        let (source, entry) = self.entry(from)?;
        let state = entry.clone();
        #[cfg(windows)]
        if state
            .permissions
            .as_ref()
            .is_some_and(Permissions::readonly)
        {
            bail!("rename source is read-only");
        }
        let bytes = state
            .after
            .as_deref()
            .context("rename source does not exist")?;
        if expected.is_some_and(|expected| expected != bytes) {
            bail!("rename content precondition failed");
        }
        let (destination, target) = self.entry(to)?;
        if source == destination {
            if from.file_name() != to.file_name() {
                bail!("case-only renames are unsupported; choose a distinct destination name");
            }
            return Ok(());
        }
        if target.after.is_some() || target.before.is_some() {
            bail!(
                "rename destination must be absent at the start of the plan: {}",
                to.display()
            );
        }
        target.after = state.after;
        target.permissions = state.permissions;
        target.origin = state.origin;
        self.entries.get_mut(&source).expect("source loaded").after = None;
        Ok(())
    }
    /// Legacy batch preparation supplies bytes already validated against this snapshot.
    pub fn stage(&mut self, path: &Path, original: &[u8], after: Vec<u8>) -> Result<()> {
        let (_, entry) = self.entry(path)?;
        if entry.before.as_deref() != Some(original) {
            bail!("file changed during preparation: {}", path.display());
        }
        entry.after = Some(after);
        Ok(())
    }
    fn changed(&self) -> BTreeMap<PathBuf, &Entry> {
        self.entries
            .iter()
            .filter(|(_, e)| e.before != e.after)
            .map(|(p, e)| (p.clone(), e))
            .collect()
    }
    pub fn preview(&self) -> Report {
        let changes = self.changed();
        let mut diff = String::new();
        for (path, entry) in &changes {
            let kind = if entry.before.is_none() {
                "create"
            } else if entry.after.is_none() {
                "delete"
            } else {
                "update"
            };
            let _ = writeln!(diff, "# {kind} {}", path.display());
            diff.push_str(&super::diff::unified_diff(
                path,
                entry.before.as_deref().unwrap_or_default(),
                entry.after.as_deref().unwrap_or_default(),
            ));
        }
        Report {
            files: changes.into_keys().collect(),
            diff,
            journal: None,
        }
    }
    pub fn commit(&self, backup: bool) -> Result<Report> {
        self.commit_with_hook(backup, &mut |_| Ok(()))
    }
    fn commit_with_hook(
        &self,
        backup: bool,
        hook: &mut dyn FnMut(usize) -> Result<()>,
    ) -> Result<Report> {
        // Validate every observed input, even no-op/read-only dependencies.
        for (path, entry) in &self.entries {
            if snapshot(path)? != entry.before {
                bail!("file changed before commit: {}", path.display());
            }
        }
        let changes = self.changed();
        if changes.is_empty() {
            return Ok(Report::default());
        }
        if backup {
            for path in changes.keys() {
                if self.entries.contains_key(&atomic::bak_path(path)) {
                    bail!("backup path is also a plan input: {}", path.display());
                }
            }
        }
        let journal = Journal::save(&changes)?;
        crate::note!(
            "xnip transaction: recovery journal {}",
            journal.directory.display()
        );
        let attempt = (|| -> Result<()> {
            if backup {
                for (path, entry) in &changes {
                    if entry.before.is_some() {
                        atomic::backup(path)?;
                    }
                }
            }
            let mut skipped = BTreeSet::new();
            let mut step = 0;
            for movement in &journal.manifest.moves {
                hook(step)?;
                step += 1;
                journal.verify_original(&movement.from)?;
                journal.verify_original(&movement.to)?;
                fs::create_dir_all(movement.to.parent().context("missing move parent")?)?;
                // Hard-link then unlink preserves identity/metadata and cannot
                // overwrite a racing destination. Cross-device moves fail safely.
                fs::hard_link(&movement.from, &movement.to)
                    .context("no-clobber rename (same filesystem required)")?;
                fs::remove_file(&movement.from)?;
                skipped.insert(movement.from.clone());
                skipped.insert(movement.to.clone());
            }
            for (path, entry) in &changes {
                if skipped.contains(path) {
                    continue;
                }
                hook(step)?;
                step += 1;
                journal.verify_original(path)?;
                install(
                    path,
                    entry.after.as_deref(),
                    entry.permissions.as_ref(),
                    entry.before.is_none(),
                )?;
            }
            Ok(())
        })();
        if let Err(error) = attempt {
            let rollback = match journal.restore(false) {
                Ok(_) => "completed".to_string(),
                Err(error) => format!("incomplete ({error:#}); inspect journal before retrying"),
            };
            return Err(CommitError {
                message: format!("{error:#}"),
                rollback,
                journal: journal.directory,
            }
            .into());
        }
        let retained = fs::remove_dir_all(&journal.directory)
            .err()
            .map(|_| journal.directory);
        Ok(Report {
            files: changes.into_keys().collect(),
            diff: String::new(),
            journal: retained,
        })
    }
}

fn install(
    path: &Path,
    bytes: Option<&[u8]>,
    permissions: Option<&Permissions>,
    absent: bool,
) -> Result<()> {
    if let Some(bytes) = bytes {
        fs::create_dir_all(path.parent().context("missing parent")?)?;
        let staged = atomic::prepare_write(path, bytes)?;
        if let Some(permissions) = permissions {
            staged.as_file().set_permissions(permissions.clone())?;
        }
        staged.as_file().sync_all()?;
        if absent {
            staged.persist_noclobber(path).map_err(|e| e.error)?;
        } else {
            staged.persist(path).map_err(|e| e.error)?;
        }
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    path: PathBuf,
    before: Option<String>,
    after: Option<String>,
    readonly: bool,
    mode: Option<u32>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Movement {
    from: PathBuf,
    to: PathBuf,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u32,
    records: Vec<Record>,
    moves: Vec<Movement>,
}
struct Journal {
    directory: PathBuf,
    manifest: Manifest,
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn synced_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = fs::File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
impl Journal {
    fn save(changes: &BTreeMap<PathBuf, &Entry>) -> Result<Self> {
        let temporary = tempfile::Builder::new()
            .prefix("xnip-transaction-")
            .tempdir()?;
        let mut records = Vec::new();
        let mut moves = Vec::new();
        for (index, (path, entry)) in changes.iter().enumerate() {
            for (suffix, bytes) in [("before", &entry.before), ("after", &entry.after)] {
                if let Some(bytes) = bytes {
                    synced_write(&temporary.path().join(format!("{index}.{suffix}")), bytes)?;
                }
            }
            #[cfg(unix)]
            let mode = {
                use std::os::unix::fs::PermissionsExt;
                entry.permissions.as_ref().map(PermissionsExt::mode)
            };
            #[cfg(not(unix))]
            let mode = None;
            records.push(Record {
                path: path.clone(),
                before: entry.before.as_deref().map(digest),
                after: entry.after.as_deref().map(digest),
                readonly: entry
                    .permissions
                    .as_ref()
                    .is_some_and(Permissions::readonly),
                mode,
            });
            if entry.origin != *path
                && entry.before.is_none()
                && let Some(source) = changes.get(&entry.origin)
                && source.after.is_none()
                && source.before == entry.after
            {
                moves.push(Movement {
                    from: entry.origin.clone(),
                    to: path.clone(),
                });
            }
        }
        let manifest = Manifest {
            version: 1,
            records,
            moves,
        };
        synced_write(
            &temporary.path().join("journal.json"),
            &serde_json::to_vec_pretty(&manifest)?,
        )?;
        #[cfg(unix)]
        fs::File::open(temporary.path())?.sync_all()?;
        Ok(Self {
            directory: temporary.keep(),
            manifest,
        })
    }
    fn load(directory: &Path) -> Result<Self> {
        let directory = fs::canonicalize(directory)?;
        let manifest: Manifest =
            serde_json::from_slice(&fs::read(directory.join("journal.json"))?)?;
        if manifest.version != 1 {
            bail!("unsupported recovery journal version");
        }
        let mut paths = BTreeSet::new();
        for record in &manifest.records {
            if !record.path.is_absolute()
                || normalize(&record.path)? != record.path
                || !paths.insert(record.path.clone())
                || record.path.starts_with(&directory)
            {
                bail!("invalid or aliased journal target");
            }
        }
        let mut moved = BTreeSet::new();
        for movement in &manifest.moves {
            if !paths.contains(&movement.from)
                || !paths.contains(&movement.to)
                || !moved.insert(&movement.from)
                || !moved.insert(&movement.to)
            {
                bail!("invalid journal move");
            }
        }
        for movement in &manifest.moves {
            let source = manifest
                .records
                .iter()
                .find(|r| r.path == movement.from)
                .context("missing move source")?;
            let target = manifest
                .records
                .iter()
                .find(|r| r.path == movement.to)
                .context("missing move target")?;
            if source.before.is_none()
                || source.after.is_some()
                || target.before.is_some()
                || target.after != source.before
            {
                bail!("invalid journal move snapshots");
            }
        }
        let journal = Self {
            directory,
            manifest,
        };
        for index in 0..journal.manifest.records.len() {
            journal.bytes(index, true)?;
            journal.bytes(index, false)?;
        }
        Ok(journal)
    }
    fn bytes(&self, index: usize, before: bool) -> Result<Option<Vec<u8>>> {
        let record = &self.manifest.records[index];
        let hash = if before {
            &record.before
        } else {
            &record.after
        };
        if let Some(hash) = hash {
            let path = self.directory.join(format!(
                "{index}.{}",
                if before { "before" } else { "after" }
            ));
            let bytes = snapshot(&path)?.context("missing journal snapshot")?;
            if digest(&bytes) != *hash {
                bail!("corrupt journal snapshot: {}", path.display());
            }
            Ok(Some(bytes))
        } else {
            Ok(None)
        }
    }
    fn verify_original(&self, path: &Path) -> Result<()> {
        let record = self
            .manifest
            .records
            .iter()
            .find(|r| r.path == path)
            .context("unrecorded target")?;
        if snapshot(path)?.as_deref().map(digest) != record.before {
            bail!("concurrent change at {}", path.display());
        }
        Ok(())
    }
    fn restore(&self, check: bool) -> Result<Report> {
        // Reject conflicts across the whole plan before restoring any path.
        for record in &self.manifest.records {
            if normalize(&record.path)? != record.path {
                bail!("recovery path changed: {}", record.path.display());
            }
            let current = snapshot(&record.path)?.as_deref().map(digest);
            if current != record.before && current != record.after {
                bail!("recovery conflict: {}", record.path.display());
            }
        }
        for movement in &self.manifest.moves {
            if !movement.from.exists() && !movement.to.exists() {
                bail!(
                    "recovery move has neither endpoint: {}",
                    movement.from.display()
                );
            }
        }
        let mut report = Report::default();
        for (index, record) in self.manifest.records.iter().enumerate() {
            let before = self.bytes(index, true)?;
            let current = snapshot(&record.path)?;
            if current != before {
                report.files.push(record.path.clone());
                report.diff.push_str(&super::diff::unified_diff(
                    &record.path,
                    current.as_deref().unwrap_or_default(),
                    before.as_deref().unwrap_or_default(),
                ));
            }
        }
        if check {
            return Ok(report);
        }
        let mut skipped = BTreeSet::new();
        for movement in self.manifest.moves.iter().rev() {
            for path in [&movement.from, &movement.to] {
                let record = self
                    .manifest
                    .records
                    .iter()
                    .find(|r| r.path == *path)
                    .context("missing recovery endpoint")?;
                let current = snapshot(path)?.as_deref().map(digest);
                if normalize(path)? != *path
                    || (current != record.before && current != record.after)
                {
                    bail!("recovery conflict at {}", path.display());
                }
            }
            if !movement.from.exists() {
                fs::hard_link(&movement.to, &movement.from)?;
            }
            if movement.to.exists() {
                fs::remove_file(&movement.to)?;
            }
            skipped.insert(&movement.from);
            skipped.insert(&movement.to);
        }
        for (index, record) in self.manifest.records.iter().enumerate().rev() {
            if skipped.contains(&record.path) {
                continue;
            }
            let before = self.bytes(index, true)?;
            let current = snapshot(&record.path)?;
            if current == before {
                continue;
            }
            if current.as_deref().map(digest) != record.after {
                bail!("recovery conflict at {}", record.path.display());
            }
            let permissions = if before.is_some() {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    Some(Permissions::from_mode(
                        record.mode.context("missing Unix permissions")?,
                    ))
                }
                #[cfg(not(unix))]
                {
                    let mut p =
                        fs::metadata(self.directory.join(format!("{index}.before")))?.permissions();
                    p.set_readonly(record.readonly);
                    Some(p)
                }
            } else {
                None
            };
            install(
                &record.path,
                before.as_deref(),
                permissions.as_ref(),
                current.is_none(),
            )?;
        }
        Ok(report)
    }
}

/// Explicit rollback of an interrupted or failed transaction. Snapshots are
/// retained so recovery is repeatable and inspectable; conflicts never force writes.
pub fn recover(directory: &Path, check: bool) -> Result<Report> {
    Journal::load(directory)?.restore(check)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn batch_lifecycle_and_no_clobber_move() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        let c = dir.path().join("new/c");
        fs::write(&a, b"original").unwrap();
        let mut plan = Plan::default();
        plan.rename(&a, &b, None).unwrap();
        plan.create(&c, b"created".to_vec()).unwrap();
        plan.commit(false).unwrap();
        assert!(!a.exists());
        assert_eq!(fs::read(b).unwrap(), b"original");
        assert_eq!(fs::read(c).unwrap(), b"created");
    }
    #[test]
    fn failed_commit_rolls_back_and_journal_is_repeatable() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        fs::write(&a, b"old").unwrap();
        let mut plan = Plan::default();
        plan.update(&a, b"new".to_vec()).unwrap();
        plan.create(&b, b"created".to_vec()).unwrap();
        let error = plan
            .commit_with_hook(false, &mut |step| {
                if step == 1 {
                    bail!("injected IO failure");
                }
                Ok(())
            })
            .unwrap_err();
        let error = error.downcast_ref::<CommitError>().unwrap();
        assert_eq!(error.rollback, "completed");
        assert_eq!(fs::read(&a).unwrap(), b"old");
        assert!(!b.exists());
        assert_eq!(
            recover(&error.journal, false).unwrap().files,
            Vec::<PathBuf>::new()
        );
        fs::remove_dir_all(&error.journal).unwrap();
    }
    #[test]
    fn recovery_restores_interrupted_move_and_detects_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        fs::write(&a, b"old").unwrap();
        let mut plan = Plan::default();
        plan.rename(&a, &b, None).unwrap();
        let journal = Journal::save(&plan.changed()).unwrap();
        fs::hard_link(&a, &b).unwrap();
        fs::remove_file(&a).unwrap();
        recover(&journal.directory, true).unwrap();
        assert!(!a.exists());
        recover(&journal.directory, false).unwrap();
        assert_eq!(fs::read(&a).unwrap(), b"old");
        assert!(!b.exists());
        fs::write(journal.directory.join("0.before"), b"corrupt").unwrap();
        assert!(recover(&journal.directory, false).is_err());
        fs::remove_dir_all(journal.directory).unwrap();
    }
    #[test]
    fn rollback_preserves_external_changes_and_retains_recovery_data() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        fs::write(&a, b"old").unwrap();
        let mut plan = Plan::default();
        plan.update(&a, b"new".to_vec()).unwrap();
        plan.create(&b, b"new".to_vec()).unwrap();
        let error = plan
            .commit_with_hook(false, &mut |step| {
                if step == 1 {
                    fs::write(&a, b"external")?;
                    bail!("injected");
                }
                Ok(())
            })
            .unwrap_err();
        let error = error.downcast_ref::<CommitError>().unwrap();
        assert!(error.rollback.starts_with("incomplete"));
        assert_eq!(fs::read(&a).unwrap(), b"external");
        assert!(recover(&error.journal, false).is_err());
        fs::remove_dir_all(&error.journal).unwrap();
    }
}
