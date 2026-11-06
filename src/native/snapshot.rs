use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, thiserror::Error)]
#[error("managed entry changed during restoration: {path}: {reason}")]
pub(super) struct RestoreConflict {
    path: PathBuf,
    reason: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Entry {
    File {
        sha256: String,
        bytes: u64,
        mode: u32,
    },
    Directory {
        mode: u32,
    },
    Symlink {
        target: PathBuf,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Snapshot {
    pub roots: Vec<PathBuf>,
    entries: BTreeMap<PathBuf, Entry>,
}

pub(super) fn atomic_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let parent = path.parent().context("manifest parent missing")?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.write_all(b"\n")?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|error| error.error)?;
    sync_directory(parent)?;
    Ok(())
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn mode(metadata: &fs::Metadata) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode()
    }
    #[cfg(not(unix))]
    {
        u32::from(metadata.permissions().readonly())
    }
}

fn set_mode(path: &Path, saved: u32) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(saved))?;
    }
    #[cfg(not(unix))]
    {
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_readonly(saved != 0);
        fs::set_permissions(path, permissions)?;
    }
    Ok(())
}

impl Snapshot {
    pub fn validate_roots(roots: &[PathBuf], profile: &Path, state: &Path) -> Result<()> {
        ensure!(!roots.is_empty(), "no managed roots declared");
        for (index, path) in roots.iter().enumerate() {
            ensure!(
                path.is_absolute() && path.starts_with(profile) && path != profile,
                "managed root must be below profile: {}",
                path.display()
            );
            ensure!(
                !path.starts_with(state) && !state.starts_with(path),
                "managed root overlaps installer state: {}",
                path.display()
            );
            ensure!(
                !path
                    .components()
                    .any(|part| matches!(part, Component::ParentDir | Component::CurDir)),
                "managed path contains traversal components"
            );
            for other in &roots[..index] {
                ensure!(
                    !path.starts_with(other) && !other.starts_with(path),
                    "overlapping managed roots"
                );
            }
            // A symlink as an ancestor would let a package manager modify paths
            // outside the declared transaction. Dependency leaf symlinks remain
            // snapshot data and are never followed by this module.
            let mut ancestor = Some(path.as_path());
            while let Some(candidate) = ancestor {
                if candidate == profile {
                    break;
                }
                if let Ok(metadata) = fs::symlink_metadata(candidate) {
                    ensure!(
                        !metadata.file_type().is_symlink(),
                        "managed root/ancestor is a symlink: {}",
                        candidate.display()
                    );
                }
                ancestor = candidate.parent();
            }
        }
        Ok(())
    }

    pub fn validate_paths(&self, profile: &Path, state: &Path) -> Result<()> {
        Self::validate_roots(&self.roots, profile, state)?;
        for (path, entry) in &self.entries {
            ensure!(
                self.roots.iter().any(|root| path.starts_with(root)),
                "snapshot path outside declared roots"
            );
            ensure!(
                !path
                    .components()
                    .any(|part| matches!(part, Component::ParentDir | Component::CurDir)),
                "snapshot path contains traversal"
            );
            if let Entry::File { sha256, .. } = entry {
                ensure!(
                    sha256.len() == 64 && sha256.bytes().all(|byte| byte.is_ascii_hexdigit()),
                    "invalid snapshot blob digest"
                );
            }
            let mut parent = path.parent();
            while let Some(ancestor) = parent {
                if self.roots.contains(&ancestor.to_path_buf())
                    || self.entries.contains_key(ancestor)
                {
                    ensure!(
                        matches!(self.entries.get(ancestor), Some(Entry::Directory { .. })),
                        "snapshot has non-directory ancestor"
                    );
                }
                if ancestor == profile {
                    break;
                }
                parent = ancestor.parent();
            }
        }
        Ok(())
    }

    pub fn validate_observed_paths(&self, state: &Path) -> Result<()> {
        for root in &self.roots {
            ensure!(
                root.is_absolute() && !root.starts_with(state),
                "invalid observed configuration path"
            );
            ensure!(
                !root
                    .components()
                    .any(|part| matches!(part, Component::ParentDir | Component::CurDir)),
                "observed path contains traversal"
            );
        }
        for path in self.entries.keys() {
            ensure!(
                self.roots.iter().any(|root| path.starts_with(root)),
                "observed entry outside roots"
            );
        }
        Ok(())
    }

    pub fn capture(roots: &[PathBuf], state: &Path) -> Result<Self> {
        Self::capture_inner(roots, state, true)
    }

    /// Expected rollback fingerprints do not need another complete backup copy.
    pub fn inspect(roots: &[PathBuf], state: &Path) -> Result<Self> {
        Self::capture_inner(roots, state, false)
    }

    fn capture_inner(roots: &[PathBuf], state: &Path, save_blobs: bool) -> Result<Self> {
        let mut entries = BTreeMap::new();
        for root in roots {
            capture_entry(root, state, &mut entries, save_blobs)?;
        }
        Ok(Self {
            roots: roots.to_vec(),
            entries,
        })
    }

    pub fn differences(&self, state: &Path) -> Result<Vec<PathBuf>> {
        let mut current = BTreeMap::new();
        for root in &self.roots {
            capture_entry(root, state, &mut current, false)?;
        }
        let mut paths = self
            .entries
            .keys()
            .chain(current.keys())
            .cloned()
            .collect::<Vec<_>>();
        paths.sort();
        paths.dedup();
        Ok(paths
            .into_iter()
            .filter(|path| self.entries.get(path) != current.get(path))
            .collect())
    }

    /// A crashed restoration may contain entries from either retained tree.
    /// Compare each entry before resuming so later user bytes remain intact.
    pub fn validate_restoration_source(&self, current: &Self, previous: &[&Self]) -> Result<()> {
        ensure!(self.roots == current.roots, "restore roots differ");
        for snapshot in previous {
            ensure!(self.roots == snapshot.roots, "restore roots differ");
        }
        let states = std::iter::once(self)
            .chain(previous.iter().copied())
            .collect::<Vec<_>>();
        let paths = states
            .iter()
            .flat_map(|snapshot| snapshot.entries.keys())
            .chain(current.entries.keys())
            .collect::<BTreeSet<_>>();
        for path in paths {
            if states
                .iter()
                .any(|snapshot| snapshot.entries.get(path) == current.entries.get(path))
            {
                continue;
            }
            bail!(RestoreConflict {
                path: path.clone(),
                reason: "entry differs from retained restoration states"
            });
        }
        Ok(())
    }

    /// Validate all content and current state before the first destructive step.
    /// `expected` is captured directly before restoration, preventing a retry from
    /// silently replacing a different user's later edits.
    pub fn restore(&self, state: &Path, expected: &Self) -> Result<()> {
        ensure!(self.roots == expected.roots, "restore roots differ");
        if let Some(path) = expected.differences(state)?.first() {
            bail!(RestoreConflict {
                path: path.clone(),
                reason: "expected content changed"
            });
        }
        self.verify_blobs(state)?;
        // Remove deepest entries first, never following symlinks.
        let mut removals = expected.entries.keys().collect::<Vec<_>>();
        removals.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
        for path in removals {
            if self.entries.get(path) == expected.entries.get(path) {
                continue;
            }
            if current_entry(path)?.as_ref() != expected.entries.get(path) {
                bail!(RestoreConflict {
                    path: path.clone(),
                    reason: "entry changed before mutation"
                });
            }
            regular_parents(path)?;
            if let Ok(metadata) = fs::symlink_metadata(path) {
                if metadata.is_dir() && !metadata.file_type().is_symlink() {
                    if !matches!(self.entries.get(path), Some(Entry::Directory { .. })) {
                        fs::remove_dir(path)?;
                    }
                } else {
                    #[cfg(windows)]
                    if metadata.permissions().readonly() {
                        let mut permissions = metadata.permissions();
                        permissions.set_readonly(false);
                        fs::set_permissions(path, permissions)?;
                    }
                    fs::remove_file(path)?;
                }
            }
        }
        // Parents sort before children. Directories receive saved permissions
        // only after children have been restored (read-only trees are valid).
        let mut entries = self.entries.iter().collect::<Vec<_>>();
        entries.sort_by_key(|(path, _)| path.components().count());
        for (path, entry) in &entries {
            if expected.entries.get(*path) == Some(*entry) {
                continue;
            }
            regular_parents(path)?;
            match entry {
                Entry::Directory { .. } => fs::create_dir_all(path)?,
                Entry::File { sha256, mode, .. } => {
                    let parent = path.parent().context("file parent absent")?;
                    fs::create_dir_all(parent)?;
                    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
                    temporary.write_all(&fs::read(state.join("blobs").join(sha256))?)?;
                    temporary.as_file().sync_all()?;
                    temporary.persist(path).map_err(|error| error.error)?;
                    set_mode(path, *mode)?;
                    sync_directory(parent)?;
                }
                Entry::Symlink { target } => create_symlink(target, path)?,
            }
        }
        for (path, entry) in entries.into_iter().rev() {
            if let Entry::Directory { mode } = entry {
                set_mode(path, *mode)?;
            }
        }
        ensure!(
            self.differences(state)?.is_empty(),
            "restoration did not reproduce baseline"
        );
        Ok(())
    }

    pub fn verify_blobs(&self, state: &Path) -> Result<()> {
        for entry in self.entries.values() {
            if let Entry::File { sha256, bytes, .. } = entry {
                let blob = state.join("blobs").join(sha256);
                ensure!(
                    !fs::symlink_metadata(&blob)?.file_type().is_symlink(),
                    "snapshot blob is a symlink"
                );
                let content = fs::read(&blob)?;
                ensure!(
                    content.len() as u64 == *bytes && digest(&content) == *sha256,
                    "snapshot blob failed integrity verification: {sha256}"
                );
            }
        }
        Ok(())
    }
}

fn current_entry(path: &Path) -> Result<Option<Entry>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let entry = if metadata.file_type().is_symlink() {
        Entry::Symlink {
            target: fs::read_link(path)?,
        }
    } else if metadata.is_dir() {
        Entry::Directory {
            mode: mode(&metadata),
        }
    } else if metadata.is_file() {
        let content = fs::read(path)?;
        Entry::File {
            sha256: digest(&content),
            bytes: content.len() as u64,
            mode: mode(&metadata),
        }
    } else {
        bail!("unexpected special managed file: {}", path.display());
    };
    Ok(Some(entry))
}

fn regular_parents(path: &Path) -> Result<()> {
    let mut parent = path.parent();
    while let Some(candidate) = parent {
        if let Ok(metadata) = fs::symlink_metadata(candidate)
            && (!metadata.is_dir() || metadata.file_type().is_symlink())
        {
            bail!(RestoreConflict {
                path: candidate.to_path_buf(),
                reason: "parent is no longer a regular directory"
            });
        }
        parent = candidate.parent();
    }
    Ok(())
}

fn capture_entry(
    path: &Path,
    state: &Path,
    entries: &mut BTreeMap<PathBuf, Entry>,
    save_blobs: bool,
) -> Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    let entry = if metadata.file_type().is_symlink() {
        Entry::Symlink {
            target: fs::read_link(path)?,
        }
    } else if metadata.is_dir() {
        Entry::Directory {
            mode: mode(&metadata),
        }
    } else if metadata.is_file() {
        let content = fs::read(path)?;
        let sha256 = digest(&content);
        if save_blobs {
            let blobs = state.join("blobs");
            fs::create_dir_all(&blobs)?;
            let blob = blobs.join(&sha256);
            if blob.exists() {
                ensure!(
                    digest(&fs::read(&blob)?) == sha256,
                    "existing snapshot blob corrupt"
                );
            } else {
                let mut temporary = tempfile::NamedTempFile::new_in(&blobs)?;
                temporary.write_all(&content)?;
                temporary.as_file().sync_all()?;
                temporary
                    .persist_noclobber(&blob)
                    .map_err(|error| error.error)?;
                sync_directory(&blobs)?;
            }
        }
        Entry::File {
            sha256,
            bytes: content.len() as u64,
            mode: mode(&metadata),
        }
    } else {
        bail!(
            "managed path is not a regular file, directory or symlink: {}",
            path.display()
        );
    };
    let recurse = matches!(entry, Entry::Directory { .. });
    entries.insert(path.to_path_buf(), entry);
    if recurse {
        let mut children = fs::read_dir(path)?
            .map(|child| child.map(|entry| entry.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        children.sort();
        for child in children {
            capture_entry(&child, state, entries, save_blobs)?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn create_symlink(target: &Path, path: &Path) -> Result<()> {
    std::os::unix::fs::symlink(target, path)?;
    Ok(())
}

#[cfg(windows)]
fn create_symlink(target: &Path, path: &Path) -> Result<()> {
    // pnpm symlinked dependency directories can require developer mode or admin.
    // Do not replace them with copied files when the OS refuses this operation.
    let resolved = if target.is_absolute() {
        target.to_path_buf()
    } else {
        path.parent()
            .context("symlink parent missing")?
            .join(target)
    };
    if resolved.is_dir() {
        std::os::windows::fs::symlink_dir(target, path)?;
    } else {
        std::os::windows::fs::symlink_file(target, path)?;
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn create_symlink(_target: &Path, _path: &Path) -> Result<()> {
    bail!("symlink restoration is unsupported on this platform")
}

fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    fs::File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
