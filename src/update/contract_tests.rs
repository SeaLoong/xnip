//! Filesystem and coordinator contracts, using real OS locks.
use super::*;

fn coordinator(root: &Path) -> Coordinator {
    Coordinator::new(Config {
        executable: root.join("xnip"),
        directory: root.join("state"),
        idle_secs: 1800,
        max_defer_secs: 86400,
        check_interval_secs: 21600,
        poll_secs: 1,
        delivery: DeliveryConfig::default(),
    })
}

#[cfg(windows)]
#[test]
fn state_replacement_preserves_an_open_readers_snapshot_without_blocking() {
    let directory = tempfile::tempdir().unwrap();
    let coordinator = coordinator(directory.path());
    coordinator.initialize().unwrap();
    let mut state = State {
        phase: "before".into(),
        ..State::default()
    };
    coordinator.save(&state).unwrap();
    let path = coordinator.config.directory.join("state.json");
    let previous = fs::read(&path).unwrap();
    let mut reader = File::open(&path).unwrap();
    state.phase = "after".into();
    coordinator.save(&state).unwrap();
    let mut snapshot = Vec::new();
    reader.read_to_end(&mut snapshot).unwrap();
    assert_eq!(snapshot, previous);
    assert_eq!(coordinator.read_state().unwrap().phase, "after");
    assert_eq!(
        fs::read_dir(&coordinator.config.directory).unwrap().count(),
        2,
        "only the state file and leases directory should remain"
    );
}

#[test]
fn a_large_poll_interval_cannot_sleep_past_a_known_update_deadline() {
    let directory = tempfile::tempdir().unwrap();
    let mut coordinator = coordinator(directory.path());
    coordinator.config.poll_secs = 7200;
    let mut state = State {
        next_check_at: 10000,
        pending: Some(Pending {
            prepared: delivery::Prepared {
                version: "9.0.0".into(),
                path: directory.path().join("candidate"),
                sha256: String::new(),
            },
            first_seen_at: 100,
        }),
        ..State::default()
    };
    coordinator.config.max_defer_secs = 20;
    assert_eq!(coordinator.poll_delay(&state, 110), Duration::from_secs(10));
    state.next_check_at = 115;
    assert_eq!(coordinator.poll_delay(&state, 110), Duration::from_secs(5));
    state.last_install_attempt_at = Some(120);
    state.next_check_at = 10000;
    assert_eq!(
        coordinator.poll_delay(&state, 121),
        Duration::from_secs(299)
    );
}

#[test]
fn a_busy_state_gate_never_blocks_a_session_or_loses_its_locked_lease() {
    let directory = tempfile::tempdir().unwrap();
    let coordinator = coordinator(directory.path());
    coordinator.initialize().unwrap();
    let gate = coordinator.gate().unwrap().unwrap();
    let start = std::time::Instant::now();
    let session = coordinator.begin(Usage::Mcp, now()).unwrap();
    assert!(start.elapsed() < Duration::from_secs(1));
    assert!(
        session.start_worker,
        "a new MCP still needs a monitor when registration races"
    );
    let mut state = State::default();
    let live = coordinator.sweep(&mut state, now()).unwrap();
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].generation, 0);
    assert!(session.path.is_file());
    assert!(session.path.with_extension("meta").is_file());
    drop(session);
    assert!(coordinator.sweep(&mut state, now()).unwrap().is_empty());
    assert!(
        fs::read_dir(coordinator.config.directory.join("leases"))
            .unwrap()
            .next()
            .is_none()
    );
    drop(gate);
}

#[test]
fn cli_end_activity_survives_a_worker_that_exited_before_the_command_ended() {
    let directory = tempfile::tempdir().unwrap();
    let coordinator = coordinator(directory.path());
    let timestamp = now();
    let session = coordinator
        .begin(Usage::Cli, timestamp.saturating_sub(7200))
        .unwrap();
    drop(session);
    let state = coordinator.read_state().unwrap();
    assert!(state.last_cli_activity_at >= timestamp);
    assert!(state.last_activity_at >= timestamp);
    let session = coordinator.begin(Usage::Mcp, now()).unwrap();
    let mut state = coordinator.read_state().unwrap();
    state.pending = Some(Pending {
        prepared: delivery::Prepared {
            version: "9.0.0".into(),
            path: directory.path().join("candidate"),
            sha256: String::new(),
        },
        first_seen_at: now(),
    });
    assert_eq!(state.install_reason(&[], now(), 86400), None);
    drop(session);
}

#[test]
fn a_pending_release_already_present_in_the_running_version_is_not_reinstalled() {
    let directory = tempfile::tempdir().unwrap();
    let coordinator = coordinator(directory.path());
    coordinator.initialize().unwrap();
    let candidate = coordinator.config.directory.join("candidate-obsolete");
    fs::write(&candidate, b"obsolete").unwrap();
    let mut state = State {
        pending: Some(Pending {
            prepared: delivery::Prepared {
                version: crate::VERSION.into(),
                path: candidate.clone(),
                sha256: String::new(),
            },
            first_seen_at: 1,
        }),
        last_install_attempt_at: Some(2),
        ..State::default()
    };
    coordinator.prune_obsolete_candidate(&mut state);
    assert!(state.pending.is_none());
    assert!(state.last_install_attempt_at.is_none());
    assert!(!candidate.exists());
}

#[test]
fn cleanup_only_removes_recorded_files_in_the_correct_namespace() {
    let directory = tempfile::tempdir().unwrap();
    let coordinator = coordinator(directory.path());
    coordinator.initialize().unwrap();
    let owned = coordinator.config.directory.join("candidate-owned");
    let unrelated = coordinator.config.directory.join("important");
    let external = directory.path().join("candidate-external");
    for path in [&owned, &unrelated, &external] {
        fs::write(path, b"keep").unwrap();
    }
    for path in [&owned, &unrelated, &external] {
        coordinator.discard_candidate(&delivery::Prepared {
            version: "9.0.0".into(),
            path: path.clone(),
            sha256: String::new(),
        });
    }
    assert!(!owned.exists());
    assert_eq!(fs::read(&unrelated).unwrap(), b"keep");
    assert_eq!(fs::read(&external).unwrap(), b"keep");
    let old = directory.path().join(".xnip-old-owned");
    fs::write(&old, b"old").unwrap();
    let mut state = State {
        retained_backups: vec![old.clone(), unrelated.clone()],
        ..State::default()
    };
    coordinator.cleanup_backups(&mut state);
    assert!(!old.exists());
    assert!(state.retained_backups.is_empty());
    assert_eq!(fs::read(unrelated).unwrap(), b"keep");
}

#[test]
fn corrupt_or_oversized_state_can_record_a_silent_retryable_failure() {
    let directory = tempfile::tempdir().unwrap();
    let coordinator = coordinator(directory.path());
    coordinator.initialize().unwrap();
    let state = coordinator.config.directory.join("state.json");
    fs::write(&state, b"not JSON").unwrap();
    assert_eq!(coordinator.read_state().unwrap().phase, "state_reset");
    fs::write(&state, vec![b'x'; MAX_STATE_BYTES as usize + 1]).unwrap();
    assert!(coordinator.read_state().is_err());
    coordinator.remember_error("registration_failed", &anyhow::anyhow!("fixture error"));
    let state = coordinator.read_state().unwrap();
    assert_eq!(state.phase, "registration_failed");
    assert_eq!(state.last_error.as_deref(), Some("fixture error"));
}
