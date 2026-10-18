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
use std::sync::Arc;

#[derive(Clone)]
struct Entry {
    before: Option<Arc<Vec<u8>>>,
    after: Option<Arc<Vec<u8>>>,
    permissions: Option<Permissions>,
    origin: PathBuf,
}

#[derive(Default)]
pub struct Plan {
    entries: BTreeMap<PathBuf, Entry>,
    #[cfg(windows)]
    identities: BTreeMap<PathBuf, PathBuf>,
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

/// Stable comparison key using Windows' one-to-one OS case mapping.
/// Keep raw UTF-16 paths intact, including unpaired surrogates.
pub(crate) fn path_identity(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::{OsStrExt, OsStringExt};
        let wide: Vec<_> = path.as_os_str().encode_wide().collect();
        if wide.is_empty() || wide.contains(&0) {
            return path.to_path_buf();
        }
        let mut identity = winsafe::WString::from_wchars_slice(&wide);
        identity.make_uppercase();
        PathBuf::from(std::ffi::OsString::from_wide(
            &identity.as_slice()[..identity.str_len()],
        ))
    }
    #[cfg(not(windows))]
    {
        path.to_path_buf()
    }
}

fn same_path(left: &Path, right: &Path) -> bool {
    path_identity(left) == path_identity(right)
}

fn content_bytes(bytes: Option<&Arc<Vec<u8>>>) -> Option<&[u8]> {
    bytes.map(|bytes| bytes.as_slice())
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
        if path
            .as_os_str()
            .as_encoded_bytes()
            .last()
            .is_some_and(|byte| matches!(byte, b'/' | b'\\'))
        {
            bail!("file target must not end with a directory separator");
        }
        let key = normalize(path)?;
        #[cfg(windows)]
        let key = {
            let identity = path_identity(&key);
            self.identities.entry(identity).or_insert(key).clone()
        };
        if !self.entries.contains_key(&key) {
            let before = snapshot(&key)?.map(Arc::new);
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
        let (_, entry) = self.entry(path)?;
        entry
            .after
            .as_ref()
            .map(|bytes| bytes.as_ref().clone())
            .with_context(|| format!("file does not exist in plan: {}", path.display()))
    }
    /// Transform current virtual bytes without cloning the complete input first.
    /// Failed transformations leave the prior virtual content available.
    pub fn transform(
        &mut self,
        path: &Path,
        transform: impl FnOnce(&[u8]) -> Result<Vec<u8>>,
    ) -> Result<()> {
        let (_, entry) = self.entry(path)?;
        let before =
            content_bytes(entry.after.as_ref()).context("transform target does not exist")?;
        let after = transform(before)?;
        entry.after = Some(Arc::new(after));
        Ok(())
    }
    /// Guard before modifying the copy-on-write virtual bytes. Unique edits reuse
    /// the current allocation after its first change; original snapshots stay immutable.
    pub fn replace_exact(
        &mut self,
        path: &Path,
        old: &[u8],
        new: &[u8],
        all: bool,
    ) -> Result<usize> {
        let (_, entry) = self.entry(path)?;
        let before = content_bytes(entry.after.as_ref())
            .context("exact replacement target does not exist")?;
        if old == new {
            return Ok(super::edit::count_exact(before, old, all)?);
        }
        if all {
            let (after, count) = super::edit::replace_exact(before, old, new, true)?;
            entry.after = Some(Arc::new(after));
            return Ok(count);
        }
        let start = super::edit::locate_exact(before, old, false)?;
        let end = start + old.len();
        if start == 0 && end == before.len() {
            // A complete replacement already supplies every output byte.
            entry.after = Some(Arc::new(new.to_vec()));
            return Ok(1);
        }
        let current = Arc::make_mut(entry.after.as_mut().expect("current bytes validated"));
        if old.len() == new.len() {
            current[start..end].copy_from_slice(new);
        } else {
            drop(current.splice(start..end, new.iter().copied()));
        }
        Ok(1)
    }

    pub fn update(&mut self, path: &Path, bytes: Vec<u8>) -> Result<()> {
        let (_, entry) = self.entry(path)?;
        if entry.after.is_none() {
            bail!("update target does not exist: {}", path.display());
        }
        entry.after = Some(Arc::new(bytes));
        Ok(())
    }
    /// Explicit whole-file upsert, optionally guarded by the current complete bytes.
    pub fn write(&mut self, path: &Path, bytes: Vec<u8>, expected: Option<&[u8]>) -> Result<()> {
        reject_link(path)?;
        let (_, entry) = self.entry(path)?;
        if let Some(expected) = expected {
            let current = content_bytes(entry.after.as_ref())
                .context("write content precondition failed: target does not exist")?;
            if current != expected {
                bail!("write content precondition failed");
            }
        }
        entry.after = Some(Arc::new(bytes));
        Ok(())
    }
    pub fn create(&mut self, path: &Path, bytes: Vec<u8>) -> Result<()> {
        reject_link(path)?;
        let (_, entry) = self.entry(path)?;
        if entry.after.is_some() {
            bail!("create target already exists: {}", path.display());
        }
        entry.after = Some(Arc::new(bytes));
        Ok(())
    }
    pub fn delete(&mut self, path: &Path, expected: Option<&[u8]>) -> Result<()> {
        reject_link(path)?;
        let (_, entry) = self.entry(path)?;
        let bytes = content_bytes(entry.after.as_ref()).context("delete target does not exist")?;
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
        let bytes = content_bytes(state.after.as_ref()).context("rename source does not exist")?;
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
        if content_bytes(entry.before.as_ref()) != Some(original) {
            bail!("file changed during preparation: {}", path.display());
        }
        entry.after = Some(Arc::new(after));
        Ok(())
    }
    fn changed(&self) -> BTreeMap<PathBuf, &Entry> {
        self.entries
            .iter()
            .filter(|(_, e)| e.before != e.after)
            .map(|(p, e)| (p.clone(), e))
            .collect()
    }
    /// Changed paths without constructing a diff for checks or error receipts.
    pub fn changed_files(&self) -> Vec<PathBuf> {
        self.entries
            .iter()
            .filter(|(_, entry)| entry.before != entry.after)
            .map(|(path, _)| path.clone())
            .collect()
    }

    pub fn preview(&self) -> Report {
        let changes = self.changed();
        let mut diff = String::new();
        let mut moved = BTreeSet::new();
        for (path, entry) in &changes {
            if entry.origin != *path
                && entry.before.is_none()
                && let Some(source) = changes.get(&entry.origin)
                && source.after.is_none()
            {
                let _ = writeln!(
                    diff,
                    "# rename {} -> {}",
                    entry.origin.display(),
                    path.display()
                );
                diff.push_str(&super::diff::unified_diff(
                    path,
                    content_bytes(source.before.as_ref()).unwrap_or_default(),
                    content_bytes(entry.after.as_ref()).unwrap_or_default(),
                ));
                moved.insert(&entry.origin);
                moved.insert(path);
            }
        }
        for (path, entry) in &changes {
            if moved.contains(path) {
                continue;
            }
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
                content_bytes(entry.before.as_ref()).unwrap_or_default(),
                content_bytes(entry.after.as_ref()).unwrap_or_default(),
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
            if !same_path(&normalize(path)?, path)
                || snapshot(path)?.as_deref() != content_bytes(entry.before.as_ref())
            {
                bail!("file changed before commit: {}", path.display());
            }
        }
        let changes = self.changed();
        if changes.is_empty() {
            return Ok(Report::default());
        }
        if backup {
            for path in changes.keys() {
                let backup_path = normalize(&atomic::bak_path(path))?;
                if self
                    .entries
                    .keys()
                    .any(|candidate| same_path(candidate, &backup_path))
                {
                    bail!("backup path is also a plan input: {}", path.display());
                }
            }
        }
        let journal = Journal::save(&changes)?;
        crate::trace!(
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
                    content_bytes(entry.after.as_ref()),
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
        if let Some(permissions) = permissions
            && staged.as_file().metadata()?.permissions() != *permissions
        {
            // prepare_write already synced unchanged permissions. Only a restored
            // snapshot mode requires another metadata update and flush.
            staged.as_file().set_permissions(permissions.clone())?;
            staged.as_file().sync_all()?;
        }
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
                before: content_bytes(entry.before.as_ref()).map(digest),
                after: content_bytes(entry.after.as_ref()).map(digest),
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
        let mut identities = BTreeSet::new();
        let journal_identity = path_identity(&directory);
        for record in &manifest.records {
            if !record.path.is_absolute()
                || !same_path(&normalize(&record.path)?, &record.path)
                || !identities.insert(path_identity(&record.path))
                || !paths.insert(record.path.clone())
                || path_identity(&record.path).starts_with(&journal_identity)
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
        if !same_path(&normalize(path)?, path)
            || snapshot(path)?.as_deref().map(digest) != record.before
        {
            bail!("concurrent change at {}", path.display());
        }
        Ok(())
    }
    fn restore(&self, check: bool) -> Result<Report> {
        // Reject conflicts across the whole plan before restoring any path.
        for record in &self.manifest.records {
            if !same_path(&normalize(&record.path)?, &record.path) {
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
                if !same_path(&normalize(path)?, path)
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

    #[cfg(windows)]
    #[test]
    fn unicode_path_identity_matches_windows_without_expanding_or_losing_utf16() {
        use std::os::windows::ffi::{OsStrExt, OsStringExt};
        for (left, right) in [("Ä", "ä"), ("Σ", "σ")] {
            assert_eq!(
                path_identity(Path::new(left)),
                path_identity(Path::new(right))
            );
        }
        for (left, right) in [("Σ", "ς"), ("ſ", "S"), ("ß", "SS"), ("\u{212a}", "k")] {
            assert_ne!(
                path_identity(Path::new(left)),
                path_identity(Path::new(right))
            );
        }
        assert_eq!(path_identity(Path::new("")), Path::new(""));
        // Lone surrogates are valid OsString data and must not collapse to U+FFFD.
        let original = std::ffi::OsString::from_wide(&[0xd800, u16::from(b'a')]);
        let identity = path_identity(Path::new(&original));
        assert_eq!(
            identity.as_os_str().encode_wide().collect::<Vec<_>>(),
            [0xd800, u16::from(b'A')]
        );
        assert_ne!(identity, path_identity(Path::new("�a")));
    }

    #[cfg(windows)]
    #[test]
    fn unicode_aliases_share_virtual_state_and_reject_backup_collisions_before_writes() {
        for (left, right) in [("Ä", "ä"), ("Σ", "σ")] {
            let directory = tempfile::tempdir().unwrap();
            let first = directory.path().join(format!("{left}.txt"));
            let alias = directory.path().join(format!("{right}.txt"));
            let mut plan = Plan::default();
            plan.create(&first, b"first".to_vec()).unwrap();
            assert!(plan.create(&alias, b"clobber".to_vec()).is_err());
            assert_eq!(plan.read(&alias).unwrap(), b"first");
            plan.write(&alias, b"last".to_vec(), Some(b"first"))
                .unwrap();
            assert_eq!(plan.changed_files().len(), 1);
            plan.commit(false).unwrap();
            assert_eq!(fs::read(&first).unwrap(), b"last");
            assert_eq!(fs::read(&alias).unwrap(), b"last");

            let mut plan = Plan::default();
            plan.write(&first, b"changed".to_vec(), Some(b"last"))
                .unwrap();
            let backup_alias = alias.with_file_name(format!("{right}.TXT.BAK"));
            plan.write(&backup_alias, b"reserved input".to_vec(), None)
                .unwrap();
            assert!(plan.commit(true).is_err());
            assert_eq!(fs::read(&first).unwrap(), b"last");
            assert!(!backup_alias.exists());
        }
    }

    #[cfg(windows)]
    #[test]
    fn recovery_rejects_unicode_duplicate_targets_and_case_aliased_journal_targets() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("Ä.txt");
        fs::write(&path, b"original").unwrap();
        let mut plan = Plan::default();
        plan.write(&path, b"new".to_vec(), None).unwrap();
        let error = plan
            .commit_with_hook(false, &mut |_| bail!("fixture failure"))
            .unwrap_err();
        let error = error.downcast_ref::<CommitError>().unwrap();
        let manifest_path = error.journal.join("journal.json");
        let original: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        let mut aliased = original.clone();
        let mut duplicate = aliased["records"][0].clone();
        duplicate["path"] = serde_json::to_value(directory.path().join("ä.txt")).unwrap();
        aliased["records"].as_array_mut().unwrap().push(duplicate);
        fs::write(&manifest_path, serde_json::to_vec(&aliased).unwrap()).unwrap();
        assert!(
            format!("{:#}", recover(&error.journal, false).unwrap_err())
                .contains("aliased journal target")
        );
        assert_eq!(fs::read(&path).unwrap(), b"original");
        let mut internal = original;
        internal["records"][0]["path"] =
            serde_json::to_value(path_identity(&error.journal.join("0.before"))).unwrap();
        fs::write(&manifest_path, serde_json::to_vec(&internal).unwrap()).unwrap();
        assert!(
            format!("{:#}", recover(&error.journal, false).unwrap_err())
                .contains("aliased journal target")
        );
        assert_eq!(fs::read(path).unwrap(), b"original");
        fs::remove_dir_all(&error.journal).unwrap();
    }

    #[test]
    fn explicit_write_checks_virtual_whole_bytes_and_distinguishes_missing_empty_files() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("nested/file");
        let mut plan = Plan::default();
        assert!(plan.write(&path, Vec::new(), Some(b"")).is_err());
        assert_eq!(plan.changed_files(), [] as [PathBuf; 0]);
        assert!(!path.parent().unwrap().exists());
        plan.write(&path, Vec::new(), None).unwrap();
        assert_eq!(plan.read(&path).unwrap(), b"");
        assert!(!path.parent().unwrap().exists());
        assert_eq!(plan.preview().files, vec![normalize(&path).unwrap()]);
        plan.write(&path, b"first".to_vec(), Some(b"")).unwrap();
        assert!(plan.write(&path, b"bad".to_vec(), Some(b"stale")).is_err());
        assert_eq!(plan.read(&path).unwrap(), b"first");
        plan.write(&path, b"\0\xff\r\nEOF".to_vec(), Some(b"first"))
            .unwrap();
        plan.commit(false).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"\0\xff\r\nEOF");
        let mut plan = Plan::default();
        plan.write(&path, b"\0\xff\r\nEOF".to_vec(), Some(b"\0\xff\r\nEOF"))
            .unwrap();
        assert_eq!(plan.changed_files(), [] as [PathBuf; 0]);
        plan.commit(true).unwrap();
        assert!(!atomic::bak_path(&path).exists());
        let empty = directory.path().join("empty");
        let mut plan = Plan::default();
        plan.write(&empty, Vec::new(), None).unwrap();
        plan.commit(false).unwrap();
        assert_eq!(fs::read(empty).unwrap(), b"");
    }

    #[test]
    fn ordered_equal_length_exact_edits_preserve_original_snapshot_and_backups() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("a");
        let mut original = String::new();
        let mut expected = String::new();
        for index in 0..128 {
            writeln!(original, "slot-{index:03}:old\r").unwrap();
            writeln!(expected, "slot-{index:03}:NEW\r").unwrap();
        }
        let original = original.into_bytes();
        let expected = expected.into_bytes();
        fs::write(&path, &original).unwrap();
        let mut plan = Plan::default();
        for index in 0..128 {
            let old = format!("slot-{index:03}:old");
            let new = format!("slot-{index:03}:NEW");
            assert_eq!(
                plan.replace_exact(&path, old.as_bytes(), new.as_bytes(), false)
                    .unwrap(),
                1
            );
        }
        assert_eq!(plan.read(&path).unwrap(), expected);
        assert_eq!(fs::read(&path).unwrap(), original);
        plan.commit(true).unwrap();
        assert_eq!(fs::read(&path).unwrap(), expected);
        assert_eq!(fs::read(atomic::bak_path(&path)).unwrap(), original);
    }

    #[test]
    fn exact_in_place_growth_shrink_deletion_binary_and_failed_guards_keep_prior_state() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("a");
        fs::write(&path, b"\0\xffBEGIN\r\nmiddle\nEOF").unwrap();
        let mut plan = Plan::default();
        plan.replace_exact(&path, b"BEGIN", b"M", false).unwrap();
        assert_eq!(plan.read(&path).unwrap(), b"\0\xffM\r\nmiddle\nEOF");
        plan.replace_exact(&path, b"M", "🙂long".as_bytes(), false)
            .unwrap();
        assert_eq!(
            plan.read(&path).unwrap(),
            [
                b"\0\xff".as_slice(),
                "🙂long".as_bytes(),
                b"\r\nmiddle\nEOF"
            ]
            .concat()
        );
        assert_eq!(plan.replace_exact(&path, b"EOF", b"", false).unwrap(), 1);
        let current = [b"\0\xff".as_slice(), "🙂long".as_bytes(), b"\r\nmiddle\n"].concat();
        assert_eq!(plan.read(&path).unwrap(), current);
        for old in [b"".as_slice(), b"missing", b"l"] {
            assert!(plan.replace_exact(&path, old, b"BAD", false).is_err());
            assert_eq!(plan.read(&path).unwrap(), current);
        }
        assert_eq!(fs::read(&path).unwrap(), b"\0\xffBEGIN\r\nmiddle\nEOF");
        plan.commit(false).unwrap();
        assert_eq!(fs::read(path).unwrap(), current);
    }

    #[test]
    fn exact_noops_and_all_keep_nonoverlapping_counts_and_validation() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("a");
        fs::write(&path, b"aaaa").unwrap();
        let mut plan = Plan::default();
        assert!(plan.replace_exact(&path, b"aa", b"aa", false).is_err());
        assert_eq!(plan.replace_exact(&path, b"aa", b"aa", true).unwrap(), 2);
        assert_eq!(plan.changed_files(), [] as [PathBuf; 0]);
        plan.commit(true).unwrap();
        assert!(!atomic::bak_path(&path).exists());
        assert_eq!(plan.replace_exact(&path, b"aa", b"$1\0", true).unwrap(), 2);
        assert_eq!(plan.read(&path).unwrap(), b"$1\0$1\0");
        assert_eq!(
            plan.replace_exact(&path, b"$1\0$1\0", b"end", false)
                .unwrap(),
            1
        );
        assert_eq!(plan.read(&path).unwrap(), b"end");
        plan.commit(false).unwrap();
        assert_eq!(fs::read(path).unwrap(), b"end");
    }

    #[test]
    fn exact_edits_after_rename_cannot_mutate_original_recovery_snapshots() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("a");
        let target = directory.path().join("b");
        fs::write(&source, b"original\r\nEOF").unwrap();
        let mut plan = Plan::default();
        plan.rename(&source, &target, Some(b"original\r\nEOF"))
            .unwrap();
        plan.replace_exact(&target, b"original", b"changed!", false)
            .unwrap();
        plan.replace_exact(&target, b"EOF", b"tail", false).unwrap();
        assert_eq!(plan.read(&target).unwrap(), b"changed!\r\ntail");
        assert_eq!(fs::read(&source).unwrap(), b"original\r\nEOF");
        let error = plan
            .commit_with_hook(false, &mut |step| {
                if step == 1 {
                    bail!("injected failure");
                }
                Ok(())
            })
            .unwrap_err();
        let error = error.downcast_ref::<CommitError>().unwrap();
        assert_eq!(error.rollback, "completed");
        assert_eq!(fs::read(source).unwrap(), b"original\r\nEOF");
        assert!(!target.exists());
        fs::remove_dir_all(&error.journal).unwrap();
    }

    #[test]
    fn transform_failures_and_owned_reads_cannot_change_the_virtual_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("a");
        fs::write(&path, b"original").unwrap();
        let mut plan = Plan::default();
        let mut owned = plan.read(&path).unwrap();
        owned[0] = b'X';
        assert_eq!(plan.read(&path).unwrap(), b"original");
        assert!(plan.transform(&path, |_| bail!("rejected")).is_err());
        assert_eq!(plan.read(&path).unwrap(), b"original");
        plan.transform(&path, |before| {
            assert_eq!(before, b"original");
            Ok(b"next".to_vec())
        })
        .unwrap();
        assert_eq!(plan.read(&path).unwrap(), b"next");
        assert_eq!(fs::read(&path).unwrap(), b"original");
        fs::write(&path, b"external").unwrap();
        assert!(plan.commit(false).is_err());
        assert_eq!(fs::read(path).unwrap(), b"external");
    }

    #[test]
    fn write_refuses_directories_and_racing_new_targets() {
        let directory = tempfile::tempdir().unwrap();
        let mut plan = Plan::default();
        assert!(plan.write(directory.path(), b"bad".to_vec(), None).is_err());
        let path = directory.path().join("new");
        plan.write(&path, b"planned".to_vec(), None).unwrap();
        fs::write(&path, b"external").unwrap();
        assert!(plan.commit(false).is_err());
        assert_eq!(fs::read(path).unwrap(), b"external");
    }

    #[cfg(unix)]
    #[test]
    fn write_never_follows_live_or_dangling_final_symlinks() {
        use std::os::unix::fs::symlink;
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("target");
        let link = directory.path().join("link");
        fs::write(&target, b"original").unwrap();
        symlink(&target, &link).unwrap();
        let mut plan = Plan::default();
        assert!(plan.write(&link, b"bad".to_vec(), None).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"original");
        fs::remove_file(&target).unwrap();
        assert!(plan.write(&link, b"bad".to_vec(), None).is_err());
        assert!(!target.exists());
    }

    #[test]
    fn failure_at_each_commit_step_restores_every_original_and_retains_repeatable_recovery() {
        for failure_step in 0..3 {
            let directory = tempfile::tempdir().unwrap();
            let paths = ["a", "b", "c"].map(|name| directory.path().join(name));
            fs::write(&paths[0], b"original").unwrap();
            fs::write(&paths[1], b"delete me").unwrap();
            let mut plan = Plan::default();
            plan.update(&paths[0], b"updated".to_vec()).unwrap();
            plan.delete(&paths[1], None).unwrap();
            plan.create(&paths[2], b"created".to_vec()).unwrap();
            let error = plan
                .commit_with_hook(false, &mut |step| {
                    if step == failure_step {
                        bail!("injected failure at {step}");
                    }
                    Ok(())
                })
                .unwrap_err();
            let error = error.downcast_ref::<CommitError>().unwrap();
            assert_eq!(error.rollback, "completed");
            assert_eq!(fs::read(&paths[0]).unwrap(), b"original");
            assert_eq!(fs::read(&paths[1]).unwrap(), b"delete me");
            assert!(!paths[2].exists());
            for check in [true, false, false] {
                assert_eq!(
                    recover(&error.journal, check).unwrap().files,
                    [] as [PathBuf; 0]
                );
            }
            fs::remove_dir_all(&error.journal).unwrap();
        }
    }

    #[test]
    fn recovery_of_interrupted_move_checks_integrity_and_preserves_preview_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let from = directory.path().join("from");
        let to = directory.path().join("to");
        fs::write(&from, b"unchanged payload").unwrap();
        let mut plan = Plan::default();
        plan.rename(&from, &to, None).unwrap();
        let preview = plan.preview();
        assert_eq!(preview.files.len(), 2);
        assert!(preview.diff.contains("# rename "));
        assert!(!preview.diff.contains("unchanged payload"));
        let journal = Journal::save(&plan.changed()).unwrap();
        fs::hard_link(&from, &to).unwrap();
        fs::remove_file(&from).unwrap();
        assert_eq!(recover(&journal.directory, true).unwrap().files.len(), 2);
        assert!(!from.exists());
        assert_eq!(fs::read(&to).unwrap(), b"unchanged payload");
        recover(&journal.directory, false).unwrap();
        assert_eq!(fs::read(&from).unwrap(), b"unchanged payload");
        assert!(!to.exists());
        fs::write(journal.directory.join("0.before"), b"corrupt").unwrap();
        assert!(recover(&journal.directory, true).is_err());
        assert!(recover(&journal.directory, false).is_err());
        assert_eq!(fs::read(&from).unwrap(), b"unchanged payload");
        fs::remove_dir_all(&journal.directory).unwrap();
    }

    #[test]
    fn rollback_and_recovery_refuse_unknown_external_changes_at_every_entry() {
        for external_at in ["a", "b"] {
            let directory = tempfile::tempdir().unwrap();
            let a = directory.path().join("a");
            let b = directory.path().join("b");
            let external = directory.path().join(external_at);
            fs::write(&a, b"old").unwrap();
            let mut plan = Plan::default();
            plan.update(&a, b"new".to_vec()).unwrap();
            plan.create(&b, b"created".to_vec()).unwrap();
            let error = plan
                .commit_with_hook(false, &mut |step| {
                    if step == 1 {
                        fs::write(&external, b"external")?;
                        bail!("injected failure");
                    }
                    Ok(())
                })
                .unwrap_err();
            let error = error.downcast_ref::<CommitError>().unwrap();
            assert!(error.rollback.starts_with("incomplete"));
            assert_eq!(fs::read(&external).unwrap(), b"external");
            for check in [true, false] {
                assert!(recover(&error.journal, check).is_err());
            }
            assert_eq!(fs::read(&external).unwrap(), b"external");
            fs::remove_dir_all(&error.journal).unwrap();
        }
    }
}
