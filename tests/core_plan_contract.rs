//! Public core-plan contracts shared by native adapters and existing frontends.
use std::fs;
use std::path::{Path, PathBuf};

use xnip::core::transaction::{
    self, CancellationToken, CommitOptions, CommitResult, ControlError, FileChange, Governance,
    Plan,
};

fn changed_plan(path: &Path, text: &[u8]) -> Plan {
    let mut plan = Plan::default();
    plan.write(path, text.to_vec(), None).unwrap();
    plan
}

#[test]
fn nonwriting_plan_and_commit_receipt_share_exact_snapshots() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source");
    let absent = directory.path().join("new/sub/file");
    let before = b"title=\xe4\xb8\xad\xe6\x96\x87\r\nport=8080\r\nend";
    let after = b"title=\xe4\xb8\xad\xe6\x96\x87\r\nport=3000\r\nend";
    fs::write(&source, before).unwrap();
    let mut plan = Plan::default();
    plan.replace_exact(&source, b"8080", b"3000", false)
        .unwrap();
    plan.create(&absent, Vec::new()).unwrap();
    let snapshots = plan.snapshots();
    assert_eq!(fs::read(&source).unwrap(), before);
    assert!(!directory.path().join("new").exists());
    assert_eq!(snapshots[0].before, None);
    assert_eq!(snapshots[0].after, Some(Vec::new()));
    assert_eq!(snapshots[1].before.as_deref(), Some(before.as_slice()));
    assert_eq!(snapshots[1].after.as_deref(), Some(after.as_slice()));
    let receipt = plan
        .commit_controlled(
            CommitOptions::default(),
            &mut (),
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(receipt.written);
    assert_eq!(receipt.changes, snapshots);
    assert_eq!(fs::read(source).unwrap(), after);
    assert_eq!(fs::read(absent).unwrap(), b"");
}

#[test]
fn planning_errors_do_not_mutate_files_or_create_parent_directories() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source");
    fs::write(&source, b"same same").unwrap();
    let mut plan = Plan::default();
    plan.create(&directory.path().join("new/sub/file"), b"created".to_vec())
        .unwrap();
    assert!(
        plan.replace_exact(&source, b"same", b"changed", false)
            .is_err()
    );
    assert_eq!(fs::read(source).unwrap(), b"same same");
    assert!(!directory.path().join("new").exists());
}

#[test]
fn changed_and_unchanged_dependencies_are_revalidated_before_commit() {
    for dependency_changes in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let a = directory.path().join("a");
        let b = directory.path().join("b");
        fs::write(&a, b"a").unwrap();
        fs::write(&b, b"b").unwrap();
        let mut plan = changed_plan(&a, b"A");
        plan.write(&b, b"b".to_vec(), None).unwrap();
        fs::write(if dependency_changes { &b } else { &a }, b"external").unwrap();
        assert!(matches!(
            plan.commit_controlled(
                CommitOptions::default(),
                &mut (),
                &CancellationToken::default()
            ),
            Err(ControlError::Check(_))
        ));
        if dependency_changes {
            assert_eq!(fs::read(a).unwrap(), b"a");
            assert_eq!(fs::read(b).unwrap(), b"external");
        } else {
            assert_eq!(fs::read(a).unwrap(), b"external");
            assert_eq!(fs::read(b).unwrap(), b"b");
        }
    }
}

struct Reject {
    phase: &'static str,
    calls: Vec<&'static str>,
    audited: bool,
}

impl Reject {
    fn check(&mut self, phase: &'static str) -> anyhow::Result<()> {
        self.calls.push(phase);
        anyhow::ensure!(self.phase != phase, "rejected {phase}");
        Ok(())
    }
}

impl Governance for Reject {
    fn authorize(&mut self, _: &[FileChange]) -> anyhow::Result<()> {
        self.check("approval")
    }
    fn validate_versions(&mut self, _: &[FileChange]) -> anyhow::Result<()> {
        self.check("version")
    }
    fn snapshot(&mut self, _: &[FileChange]) -> anyhow::Result<()> {
        self.check("snapshot")
    }
    fn audit(&mut self, _: &[FileChange], result: &Result<CommitResult, ControlError>) {
        assert!(matches!(result, Err(ControlError::Governance(_))));
        self.audited = true;
    }
}

#[test]
fn approval_version_and_snapshot_denials_are_prewrite_and_audited() {
    for phase in ["approval", "version", "snapshot"] {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("file");
        let plan = changed_plan(&file, b"new");
        let mut host = Reject {
            phase,
            calls: Vec::new(),
            audited: false,
        };
        let error = plan
            .commit_controlled(
                CommitOptions::default(),
                &mut host,
                &CancellationToken::default(),
            )
            .unwrap_err();
        assert_eq!(error.kind(), "governance");
        assert!(host.audited);
        assert_eq!(host.calls.last(), Some(&phase));
        assert!(!file.exists());
    }
}

struct CancelAt {
    cancellation: CancellationToken,
    step: usize,
    audits: usize,
}

impl Governance for CancelAt {
    fn checkpoint(&mut self, step: usize, _: &[FileChange]) -> anyhow::Result<()> {
        if step == self.step {
            self.cancellation.cancel();
        }
        Ok(())
    }
    fn audit(&mut self, _: &[FileChange], result: &Result<CommitResult, ControlError>) {
        assert!(matches!(result, Err(ControlError::Partial(_))));
        self.audits += 1;
    }
}

#[test]
fn cancellation_at_each_write_and_final_checkpoint_restores_all_bytes() {
    for step in 0..=3 {
        let directory = tempfile::tempdir().unwrap();
        let a = directory.path().join("a");
        let b = directory.path().join("b");
        let c = directory.path().join("c");
        fs::write(&a, b"original").unwrap();
        fs::write(&b, b"delete").unwrap();
        let mut plan = changed_plan(&a, b"changed");
        plan.delete(&b, None).unwrap();
        plan.create(&c, b"new".to_vec()).unwrap();
        let cancellation = CancellationToken::default();
        let mut host = CancelAt {
            cancellation: cancellation.clone(),
            step,
            audits: 0,
        };
        let ControlError::Partial(error) = plan
            .commit_controlled(CommitOptions::default(), &mut host, &cancellation)
            .unwrap_err()
        else {
            panic!("cancellation must retain its rollback receipt")
        };
        assert_eq!(error.rollback, "completed");
        assert_eq!(host.audits, 1);
        assert_eq!(fs::read(&a).unwrap(), b"original");
        assert_eq!(fs::read(&b).unwrap(), b"delete");
        assert!(!c.exists());
        for check in [true, false, false] {
            assert_eq!(
                transaction::recover(&error.journal, check).unwrap().files,
                Vec::<PathBuf>::new()
            );
        }
        fs::remove_dir_all(error.journal).unwrap();
    }
}

#[test]
fn cancellation_before_commit_preserves_the_entire_nonwriting_plan() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("new/file");
    let plan = changed_plan(&path, b"new");
    let cancellation = CancellationToken::default();
    cancellation.cancel();
    assert!(matches!(
        plan.commit_controlled(CommitOptions::default(), &mut (), &cancellation),
        Err(ControlError::Cancelled)
    ));
    assert!(!directory.path().join("new").exists());
}

#[test]
fn ordered_core_plan_transforms_keep_binary_crlf_and_unterminated_eof() {
    let directory = tempfile::tempdir().unwrap();
    let from = directory.path().join("a");
    let to = directory.path().join("b");
    fs::write(&from, b"\xffa\r\nz").unwrap();
    let mut plan = Plan::default();
    plan.rename(&from, &to, None).unwrap();
    plan.transform(&to, |before| {
        Ok(xnip::core::edit::insert_exact(
            before,
            b"\xff",
            b"b",
            xnip::core::edit::Placement::After,
        )?
        .0)
    })
    .unwrap();
    plan.replace_exact(&to, b"z", b"end", false).unwrap();
    plan.commit_controlled(
        CommitOptions::default(),
        &mut (),
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(!from.exists());
    assert_eq!(fs::read(to).unwrap(), b"\xffba\r\nend");
}

#[test]
fn recovery_authorizes_all_journal_paths_including_unchanged_entries() {
    let directory = tempfile::tempdir().unwrap();
    let a = directory.path().join("a");
    let b = directory.path().join("b");
    fs::write(&a, b"original a").unwrap();
    fs::write(&b, b"original b").unwrap();
    let mut plan = changed_plan(&a, b"new a");
    plan.write(&b, b"new b".to_vec(), None).unwrap();
    let cancellation = CancellationToken::default();
    let mut host = CancelAt {
        cancellation: cancellation.clone(),
        step: 0,
        audits: 0,
    };
    let ControlError::Partial(error) = plan
        .commit_controlled(CommitOptions::default(), &mut host, &cancellation)
        .unwrap_err()
    else {
        panic!("expected a retained journal")
    };
    assert_eq!(
        transaction::recover(&error.journal, true).unwrap().files,
        Vec::<PathBuf>::new()
    );
    let mut calls = 0;
    let result = transaction::recover_authorized(&error.journal, false, &mut |paths| {
        calls += 1;
        assert_eq!(paths, [a.clone(), b.clone()]);
        anyhow::bail!("outside host roots")
    });
    assert!(result.is_err());
    assert_eq!(calls, 1);
    assert_eq!(fs::read(a).unwrap(), b"original a");
    assert_eq!(fs::read(b).unwrap(), b"original b");
    fs::remove_dir_all(error.journal).unwrap();
}

#[test]
fn cancellation_precedes_backup_side_effects() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("file");
    let backup = directory.path().join("file.bak");
    fs::write(&path, b"original").unwrap();
    fs::write(&backup, b"previous backup").unwrap();
    let plan = changed_plan(&path, b"new");
    let cancellation = CancellationToken::default();
    let mut host = CancelAt {
        cancellation: cancellation.clone(),
        step: 0,
        audits: 0,
    };
    let ControlError::Partial(error) = plan
        .commit_controlled(CommitOptions { backup: true }, &mut host, &cancellation)
        .unwrap_err()
    else {
        panic!("expected a retained rollback journal")
    };
    assert_eq!(error.rollback, "completed");
    assert_eq!(fs::read(path).unwrap(), b"original");
    assert_eq!(fs::read(backup).unwrap(), b"previous backup");
    fs::remove_dir_all(error.journal).unwrap();
}
