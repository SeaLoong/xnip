//! Updater contracts use isolated state and offline fixtures, never the installed executable.
//! Release executables deliberately have no endpoint override.
#![cfg(debug_assertions)]
use assert_cmd::cargo::CommandCargoExt;
use serde_json::{Value, json};
#[cfg(windows)]
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

fn command(root: &Path, enabled: bool) -> Command {
    let mut command = Command::cargo_bin("xnip").unwrap();
    command
        .env("XNIP_AUTO_UPDATE", if enabled { "1" } else { "0" })
        .env("XNIP_UPDATE_STATE_DIR", root)
        .stdin(Stdio::null());
    command
}

fn peek(root: &Path, enabled: bool) -> Output {
    let input = root.with_extension("input");
    std::fs::write(&input, b"unchanged\n").unwrap();
    command(root, enabled)
        .arg("peek")
        .arg(input)
        .arg("--all")
        .output()
        .unwrap()
}

struct LiveMcp {
    child: Child,
    output: Receiver<Value>,
    reader: Option<std::thread::JoinHandle<()>>,
}

impl LiveMcp {
    fn start(root: &Path, enabled: bool) -> Self {
        let mut command = command(root, enabled);
        command.arg("mcp");
        Self::from_command(command)
    }

    fn from_command(mut command: Command) -> Self {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, output) = channel();
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let line = line.unwrap();
                let frame = serde_json::from_str(&line).expect("MCP stdout must remain JSON-RPC");
                if sender.send(frame).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            output,
            reader: Some(reader),
        }
    }

    fn send(&mut self, value: &Value) {
        let input = self.child.stdin.as_mut().unwrap();
        writeln!(input, "{value}").unwrap();
        input.flush().unwrap();
    }

    fn receive(&self, id: u64) -> Value {
        loop {
            let response = self
                .output
                .recv_timeout(Duration::from_secs(10))
                .expect("MCP must respond while stdin is still open");
            if response["id"] == id {
                return response;
            }
        }
    }

    fn round_trip(&mut self) {
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "updater-contract", "version": "1"}
            }
        }));
        let initialized = self.receive(1);
        assert!(initialized.get("error").is_none(), "{initialized}");
        self.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
        self.send(&json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}));
        let listed = self.receive(2);
        assert!(
            listed["result"]["tools"]
                .as_array()
                .is_some_and(|tools| tools.len() == 5),
            "{listed}"
        );
    }

    fn stop(mut self) -> Vec<u8> {
        self.child.stdin.take();
        let _ = self.child.kill();
        self.child.wait().unwrap();
        let mut stderr = Vec::new();
        std::io::Read::read_to_end(self.child.stderr.as_mut().unwrap(), &mut stderr).unwrap();
        self.reader.take().unwrap().join().unwrap();
        stderr
    }
}

impl Drop for LiveMcp {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

struct Installation {
    temporary: tempfile::TempDir,
    binary: PathBuf,
    state_root: PathBuf,
    input: PathBuf,
}

impl Installation {
    fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let binary = temporary
            .path()
            .join(format!("xnip{}", std::env::consts::EXE_SUFFIX));
        let source = Command::cargo_bin("xnip").unwrap();
        std::fs::copy(source.get_program(), &binary).unwrap();
        let state_root = temporary.path().join("update-state");
        let input = temporary.path().join("input");
        std::fs::write(&input, b"unchanged\n").unwrap();
        Self {
            temporary,
            binary,
            state_root,
            input,
        }
    }

    fn command(&self, endpoint: &str) -> Command {
        let mut command = Command::new(&self.binary);
        command
            .env("XNIP_AUTO_UPDATE", "1")
            .env("XNIP_UPDATE_STATE_DIR", &self.state_root)
            .env("XNIP_UPDATE_RELEASE_URL", endpoint)
            .env("XNIP_UPDATE_CHECK_INTERVAL_SECS", "3600")
            .env("XNIP_UPDATE_IDLE_SECS", "1")
            .env("XNIP_UPDATE_POLL_SECS", "1")
            .env("XNIP_UPDATE_MAX_DEFER_SECS", "86400")
            .current_dir(self.temporary.path())
            .stdin(Stdio::null());
        command
    }

    fn read(&self, endpoint: &str) -> Command {
        let mut command = self.command(endpoint);
        command.arg("peek").arg(&self.input).arg("--all");
        command
    }

    fn state_directory(&self) -> Option<PathBuf> {
        std::fs::read_dir(&self.state_root)
            .ok()?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| path.join("state.json").is_file())
    }

    fn state(&self) -> Option<Value> {
        let bytes = std::fs::read(self.state_directory()?.join("state.json")).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    #[cfg(windows)]
    fn inject_pending(&self, expired: bool) -> (Vec<u8>, u64) {
        let directory = self.state_directory().unwrap();
        let mut candidate = std::fs::read(&self.binary).unwrap();
        candidate.extend_from_slice(b"\nxnip updater integration overlay\n");
        let path = directory.join("candidate-contract.exe");
        std::fs::write(&path, &candidate).unwrap();
        let digest = format!("{:x}", Sha256::digest(&candidate));
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(directory.join("state.lock"))
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            match lock.try_lock() {
                Ok(()) => break,
                Err(std::fs::TryLockError::WouldBlock) => {
                    assert!(Instant::now() < deadline, "state gate stayed busy");
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("state gate failed: {error}"),
            }
        }
        let timestamp = unix_now();
        let mut state = self.state().unwrap();
        state["pending"] = json!({
            "prepared": {"version": "999.0.0", "path": path, "sha256": digest},
            "first_seen_at": if expired { timestamp - 86_401 } else { timestamp }
        });
        state["window"] = Value::Null;
        state["next_check_at"] = json!(timestamp + 3_600);
        state["last_install_attempt_at"] = Value::Null;
        state["last_error"] = Value::Null;
        let mut staged = tempfile::NamedTempFile::new_in(&directory).unwrap();
        serde_json::to_writer(&mut staged, &state).unwrap();
        staged.persist(directory.join("state.json")).unwrap();
        (candidate, timestamp)
    }

    #[cfg(windows)]
    fn assert_installed_runs(&self, expected: &[u8]) {
        assert_eq!(std::fs::read(&self.binary).unwrap(), expected);
        let output = Command::new(&self.binary)
            .env("XNIP_AUTO_UPDATE", "0")
            .arg("--version")
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(
            std::str::from_utf8(&output.stdout).unwrap().trim(),
            format!("xnip {}", xnip::VERSION)
        );
        assert_eq!(output.stderr, b"");
    }

    fn wait_for_state(&self, predicate: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let state = self.state();
            if let Some(state) = &state
                && predicate(state)
            {
                return state.clone();
            }
            assert!(
                Instant::now() < deadline,
                "state did not converge: {state:?}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

#[cfg(windows)]
fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn fixture() -> (TcpListener, String) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/latest", listener.local_addr().unwrap());
    (listener, endpoint)
}

fn accept_request(listener: &TcpListener) -> TcpStream {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut reader = BufReader::new(&mut stream);
                let mut line = String::new();
                loop {
                    line.clear();
                    reader.read_line(&mut line).unwrap();
                    assert!(
                        !line.is_empty(),
                        "worker closed before sending HTTP headers"
                    );
                    if line == "\r\n" {
                        return stream;
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline, "background check did not start");
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("offline listener failed: {error}"),
        }
    }
}

fn fail_request(stream: &mut TcpStream) {
    stream
        .write_all(
            b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        )
        .unwrap();
    stream.flush().unwrap();
}

#[test]
fn a_blocked_background_check_is_singleton_and_survives_all_foreground_exits() {
    let installation = Installation::new();
    let (listener, endpoint) = fixture();
    let original = std::fs::read(&installation.binary).unwrap();
    let (completed, outputs) = channel();
    let mut callers = Vec::new();
    for _ in 0..8 {
        let mut command = installation.read(&endpoint);
        let completed = completed.clone();
        callers.push(std::thread::spawn(move || {
            completed.send(command.output().unwrap()).unwrap();
        }));
    }
    drop(completed);
    let mut request = accept_request(&listener);
    for _ in 0..8 {
        let output = outputs
            .recv_timeout(Duration::from_secs(10))
            .expect("foreground must exit while its background worker is still waiting for HTTP");
        assert!(output.status.success());
        assert_eq!(output.stdout, b"     1: unchanged\n");
        assert_eq!(output.stderr, b"");
    }
    for caller in callers {
        caller.join().unwrap();
    }
    assert!(
        listener
            .accept()
            .is_err_and(|error| error.kind() == std::io::ErrorKind::WouldBlock),
        "concurrent CLI invocations must share one background release check"
    );
    fail_request(&mut request);
    drop(request);
    let failed = installation.wait_for_state(|state| {
        state["last_error"]
            .as_str()
            .is_some_and(|error| !error.is_empty())
    });
    assert!(failed["last_check_at"].is_u64());
    installation.wait_for_state(|state| state["worker_pid"].is_null());
    assert_eq!(std::fs::read(&installation.binary).unwrap(), original);
}

#[test]
fn live_mcp_remains_usable_during_a_blocked_check_and_after_its_failure() {
    let installation = Installation::new();
    let (listener, endpoint) = fixture();
    let mut command = installation.command(&endpoint);
    command.arg("mcp");
    let mut rpc = LiveMcp::from_command(command);
    let mut request = accept_request(&listener);
    rpc.round_trip();
    assert!(rpc.child.try_wait().unwrap().is_none());
    fail_request(&mut request);
    drop(request);
    installation.wait_for_state(|state| {
        state["phase"] == "check_failed" && state["last_error"].as_str().is_some()
    });
    rpc.round_trip();
    assert_eq!(rpc.stop(), b"");
    installation.wait_for_state(|state| state["worker_pid"].is_null());
}

#[test]
fn a_second_worker_exits_silently_and_process_termination_releases_the_os_lock() {
    let installation = Installation::new();
    let (listener, endpoint) = fixture();
    let leader = installation
        .command(&endpoint)
        .arg("__update-worker")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Reuse the process guard so a failed assertion still terminates the worker.
    let (_sender, output) = channel();
    let mut leader = LiveMcp {
        child: leader,
        output,
        reader: None,
    };
    let request = accept_request(&listener);
    installation.wait_for_state(|state| state["worker_pid"] == leader.child.id());
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(installation.state_directory().unwrap().join("worker.lock"))
        .unwrap();
    assert!(matches!(
        lock.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    let follower = installation
        .command(&endpoint)
        .arg("__update-worker")
        .output()
        .unwrap();
    assert!(follower.status.success());
    assert_eq!(follower.stdout, b"");
    assert_eq!(follower.stderr, b"");
    assert!(
        listener
            .accept()
            .is_err_and(|error| error.kind() == std::io::ErrorKind::WouldBlock),
        "a second worker must not make another HTTP request"
    );
    leader.child.kill().unwrap();
    leader.child.wait().unwrap();
    assert!(
        lock.try_lock().is_ok(),
        "a killed worker must not leave a stale singleton lock"
    );
    drop(request);
}

#[cfg(windows)]
#[test]
fn pending_update_waits_for_mcp_to_close_and_installs_only_on_the_next_usage() {
    let installation = Installation::new();
    let (listener, endpoint) = fixture();
    let original = std::fs::read(&installation.binary).unwrap();
    let mut command = installation.command(&endpoint);
    command.arg("mcp");
    let mut rpc = LiveMcp::from_command(command);
    let mut request = accept_request(&listener);
    rpc.round_trip();
    let (candidate, injected_at) = installation.inject_pending(false);
    fail_request(&mut request);
    drop(request);
    installation.wait_for_state(|state| {
        state["last_activity_at"]
            .as_u64()
            .is_some_and(|time| time > injected_at)
            && state["phase"] == "check_failed"
    });
    assert_eq!(std::fs::read(&installation.binary).unwrap(), original);
    assert!(installation.state().unwrap()["installed_at"].is_null());
    rpc.round_trip();
    assert_eq!(rpc.stop(), b"");
    let waiting = installation.wait_for_state(|state| {
        state["phase"] == "waiting_for_next_use" && state["worker_pid"].is_null()
    });
    assert!(waiting["pending"].is_object());
    assert!(waiting["window"].is_null());
    assert_eq!(std::fs::read(&installation.binary).unwrap(), original);
    // Exercise the real spawn cooldown without changing the stored scheduling facts.
    let next_spawn_at = waiting["last_spawn_at"].as_u64().unwrap() + 3;
    let deadline = Instant::now() + Duration::from_secs(5);
    while unix_now() < next_spawn_at {
        assert!(Instant::now() < deadline, "spawn cooldown did not expire");
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = installation.read(&endpoint).output().unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"     1: unchanged\n");
    assert_eq!(output.stderr, b"");
    let installed = installation
        .wait_for_state(|state| state["phase"] == "installed" && state["worker_pid"].is_null());
    assert_eq!(installed["install_reason"], "idle_start");
    assert_eq!(installed["installed_version"], "999.0.0");
    assert!(installed["pending"].is_null());
    installation.assert_installed_runs(&candidate);
}

#[cfg(windows)]
#[test]
fn maximum_deferral_installs_during_a_live_mcp_session_and_hands_off_its_monitor() {
    let installation = Installation::new();
    let (listener, endpoint) = fixture();
    let original = std::fs::read(&installation.binary).unwrap();
    let mut command = installation.command(&endpoint);
    command.arg("mcp");
    let mut rpc = LiveMcp::from_command(command);
    let mut request = accept_request(&listener);
    rpc.round_trip();
    let previous_worker = installation.state().unwrap()["worker_pid"]
        .as_u64()
        .unwrap();
    let (candidate, _) = installation.inject_pending(true);
    fail_request(&mut request);
    drop(request);
    let installed = installation.wait_for_state(|state| {
        state["phase"] == "installed"
            && state["worker_pid"]
                .as_u64()
                .is_some_and(|pid| pid != previous_worker)
    });
    assert_eq!(installed["install_reason"], "deadline");
    assert_eq!(installed["installed_version"], "999.0.0");
    assert!(installed["pending"].is_null());
    assert_ne!(candidate, original);
    installation.assert_installed_runs(&candidate);
    assert!(rpc.child.try_wait().unwrap().is_none());
    rpc.round_trip();
    let backups = installed["retained_backups"].as_array().unwrap();
    assert!(
        !backups.is_empty(),
        "a live old MCP image must remain available"
    );
    for path in backups {
        assert_eq!(std::fs::read(path.as_str().unwrap()).unwrap(), original);
    }
    assert_eq!(rpc.stop(), b"");
    installation.wait_for_state(|state| {
        state["worker_pid"].is_null()
            && state["retained_backups"]
                .as_array()
                .is_some_and(Vec::is_empty)
    });
    for path in backups {
        assert!(!Path::new(path.as_str().unwrap()).exists());
    }
}

#[test]
fn discovery_commands_do_not_launch_workers_or_create_state() {
    let installation = Installation::new();
    let (listener, endpoint) = fixture();
    for args in [vec!["--version"], vec!["--help"], vec!["doctor"]] {
        let output = installation.command(&endpoint).args(args).output().unwrap();
        assert!(output.status.success());
        assert_eq!(output.stderr, b"");
    }
    assert!(!installation.state_root.exists());
    assert!(
        listener
            .accept()
            .is_err_and(|error| error.kind() == std::io::ErrorKind::WouldBlock)
    );
}

#[test]
fn explicit_opt_out_does_not_create_update_state_or_change_cli_output() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("absent");
    let output = peek(&root, false);
    assert!(output.status.success());
    assert_eq!(output.stdout, b"     1: unchanged\n");
    assert_eq!(output.stderr, b"");
    assert!(!root.exists());
}

#[test]
fn explicit_opt_out_keeps_a_live_mcp_session_free_of_update_state_and_messages() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("absent");
    let mut rpc = LiveMcp::start(&root, false);
    rpc.round_trip();
    assert!(!root.exists());
    assert_eq!(rpc.stop(), b"");
}

#[test]
fn unavailable_update_storage_cannot_change_cli_exit_status_or_streams() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("state-is-a-file");
    std::fs::write(&root, b"unrelated bytes\n").unwrap();
    let baseline = peek(&root, false);
    let updating = peek(&root, true);
    assert!(updating.status.success());
    assert_eq!(updating.stdout, baseline.stdout);
    assert_eq!(updating.stderr, baseline.stderr);
    assert_eq!(std::fs::read(&root).unwrap(), b"unrelated bytes\n");
}

#[test]
fn unavailable_update_storage_keeps_mcp_protocol_and_stderr_unchanged() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("state-is-a-file");
    std::fs::write(&root, b"unrelated bytes\n").unwrap();
    let mut rpc = LiveMcp::start(&root, true);
    rpc.round_trip();
    assert_eq!(rpc.stop(), b"");
    assert_eq!(std::fs::read(&root).unwrap(), b"unrelated bytes\n");
}
