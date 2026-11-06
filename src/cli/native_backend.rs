//! Versioned, local JSON-lines frontend for native host adapters.
use std::collections::HashMap;
use std::io::{BufRead, Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::core::transaction;
use crate::native::operations;

const PROTOCOL: u32 = 1;
const MAX_REQUEST_BYTES: usize = 32 * 1024 * 1024;
const MAX_PLANS: usize = 64;
const MAX_QUEUED_REQUESTS: usize = 64;

type Cancellations = Arc<Mutex<HashMap<u64, transaction::CancellationToken>>>;

struct Incoming {
    id: Value,
    action: anyhow::Result<Action>,
    cancellation_requested: bool,
}

#[derive(Debug, Deserialize)]
struct Request {
    id: Value,
    protocol: u32,
    #[serde(flatten)]
    action: Action,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum Action {
    Capabilities {},
    Read {
        path: PathBuf,
        #[serde(default)]
        offset: usize,
        #[serde(default)]
        max_bytes: Option<usize>,
        #[serde(default)]
        content_encoding: ContentEncoding,
    },
    Plan {
        operations: Vec<operations::Operation>,
        #[serde(default)]
        content_encoding: ContentEncoding,
    },
    Commit {
        plan_id: u64,
        policy: Policy,
        #[serde(default)]
        backup: bool,
    },
    Abort {
        plan_id: u64,
    },
    Recover {
        journal: PathBuf,
        #[serde(default)]
        check: bool,
        policy: Policy,
    },
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ContentEncoding {
    #[default]
    Bytes,
    Utf8,
}

impl ContentEncoding {
    fn content(self, bytes: Vec<u8>) -> Value {
        match self {
            Self::Bytes => json!(bytes),
            Self::Utf8 => match String::from_utf8(bytes) {
                Ok(text) => Value::String(text),
                Err(error) => json!(error.into_bytes()),
            },
        }
    }

    fn changes(self, changes: Vec<transaction::FileChange>) -> Value {
        Value::Array(
            changes
                .into_iter()
                .map(|change| {
                    json!({
                        "path":change.path,
                        "before":change.before.map(|bytes| self.content(bytes)),
                        "after":change.after.map(|bytes| self.content(bytes)),
                    })
                })
                .collect(),
        )
    }
}

/// Explicit process authority supplied by the host after its own approval checks.
/// A policy is not an approval token and cannot extend OS process permissions.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Policy {
    read_only: bool,
    #[serde(deserialize_with = "required_roots")]
    writable_roots: Option<Vec<PathBuf>>,
    #[serde(skip)]
    backup: bool,
}

fn required_roots<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Vec<PathBuf>>, D::Error> {
    Option::deserialize(deserializer)
}

#[derive(Serialize)]
struct Response {
    id: Value,
    protocol: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<ProtocolError>,
}

#[derive(Serialize)]
struct ProtocolError {
    code: &'static str,
    message: String,
}

struct PendingPlan {
    plan: transaction::Plan,
    cancellation: transaction::CancellationToken,
    content_encoding: ContentEncoding,
}

struct Session {
    plans: HashMap<u64, PendingPlan>,
    next_plan: u64,
    cancellations: Cancellations,
}

/// Run until EOF; stdout contains only complete JSON records, including errors.
/// Input remains live during a commit so an abort can signal its cancellation
/// token. EOF drains queued requests and lets an active transaction finish.
pub fn run() -> anyhow::Result<u8> {
    let cancellations = Cancellations::default();
    let mut session = Session {
        plans: HashMap::new(),
        next_plan: 1,
        cancellations: cancellations.clone(),
    };
    let output = Arc::new(Mutex::new(std::io::stdout()));
    let reader_output = output.clone();
    let (sender, receiver) = mpsc::sync_channel(MAX_QUEUED_REQUESTS);
    let reader = thread::spawn(move || {
        read_requests(
            std::io::BufReader::new(std::io::stdin()),
            &sender,
            &cancellations,
            &reader_output,
        )
    });
    for incoming in receiver {
        let result = incoming
            .action
            .and_then(|action| session.dispatch(action, incoming.cancellation_requested));
        emit_response(&output, &response(incoming.id, result))?;
    }
    reader
        .join()
        .map_err(|_| anyhow::anyhow!("native backend input reader panicked"))?
}

fn response(id: Value, result: anyhow::Result<Value>) -> Response {
    match result {
        Ok(result) => Response {
            id,
            protocol: PROTOCOL,
            result: Some(result),
            error: None,
        },
        Err(error) => Response {
            id,
            protocol: PROTOCOL,
            result: None,
            error: Some(ProtocolError {
                code: error
                    .downcast_ref::<transaction::ControlError>()
                    .map_or("usage", transaction::ControlError::kind),
                message: format!("{error:#}"),
            }),
        },
    }
}

fn emit_response(out: &Arc<Mutex<impl Write>>, response: &Response) -> anyhow::Result<()> {
    let mut output = out
        .lock()
        .map_err(|_| anyhow::anyhow!("native backend output lock poisoned"))?;
    write_response(&mut *output, response)
}

fn read_requests(
    mut input: impl BufRead,
    sender: &mpsc::SyncSender<Incoming>,
    cancellations: &Cancellations,
    output: &Arc<Mutex<impl Write>>,
) -> anyhow::Result<u8> {
    loop {
        let mut line = Vec::new();
        // Limit allocation even for an unterminated or malicious peer request.
        let count = (&mut input)
            .take((MAX_REQUEST_BYTES + 1) as u64)
            .read_until(b'\n', &mut line)?;
        if count == 0 {
            return Ok(crate::output::exit::SUCCESS);
        }
        if line.len() > MAX_REQUEST_BYTES {
            emit_response(
                output,
                &response(
                    Value::Null,
                    Err(anyhow::anyhow!(
                        "native backend request exceeds 32 MiB; session closed"
                    )),
                ),
            )?;
            return Ok(crate::output::exit::USAGE);
        }
        let raw: Result<Value, _> = serde_json::from_slice(&line);
        let parse_id = raw
            .as_ref()
            .ok()
            .and_then(|raw| raw.get("id"))
            .cloned()
            .unwrap_or(Value::Null);
        let request: Result<Request, _> = raw.and_then(serde_json::from_value);
        let (id, action) = match request {
            Ok(request) if request.protocol == PROTOCOL => (request.id, Ok(request.action)),
            Ok(request) => (
                request.id,
                Err(anyhow::anyhow!(
                    "unsupported native backend protocol {}; expected {PROTOCOL}",
                    request.protocol
                )),
            ),
            Err(error) => (
                parse_id,
                Err(anyhow::anyhow!("invalid native backend request: {error}")),
            ),
        };
        // This is a control signal, not a competing persistence worker. The
        // commit's next checkpoint decides whether rollback is still possible;
        // only its actual response establishes whether bytes were committed.
        let cancellation_requested = if let Ok(Action::Abort { plan_id }) = &action {
            let tokens = cancellations
                .lock()
                .map_err(|_| anyhow::anyhow!("native backend cancellation lock poisoned"))?;
            if let Some(token) = tokens.get(plan_id) {
                token.cancel();
                true
            } else {
                false
            }
        } else {
            false
        };
        let incoming = Incoming {
            id,
            action,
            cancellation_requested,
        };
        match sender.try_send(incoming) {
            Ok(()) => {}
            Err(mpsc::TrySendError::Full(incoming)) => {
                // Keep reading controls instead of blocking behind a flooded
                // request queue. Correlated responses can arrive out of order.
                let result = if incoming.cancellation_requested {
                    Ok(json!({"cancellation_requested":true}))
                } else {
                    Err(anyhow::anyhow!(
                        "native backend request queue is full; retry after an outstanding response"
                    ))
                };
                emit_response(output, &response(incoming.id, result))?;
            }
            Err(mpsc::TrySendError::Disconnected(_)) => return Ok(crate::output::exit::SUCCESS),
        }
    }
}

fn write_response(out: &mut impl Write, response: &Response) -> anyhow::Result<()> {
    serde_json::to_writer(&mut *out, &response)?;
    out.write_all(b"\n")?;
    out.flush()?;
    Ok(())
}

impl Session {
    fn dispatch(&mut self, action: Action, cancellation_requested: bool) -> anyhow::Result<Value> {
        match action {
            Action::Capabilities {} => Ok(json!({
                "api_version":"1.0.0", "ops": operations::OP_NAMES, "protocol": PROTOCOL,
                "max_request_bytes": MAX_REQUEST_BYTES, "max_pending_plans": MAX_PLANS,
                "content_encodings":["bytes","utf8"],
            })),
            Action::Read {
                path,
                offset,
                max_bytes,
                content_encoding,
            } => {
                let bytes = std::fs::read(path)?;
                let start = offset.min(bytes.len());
                let end = max_bytes.map_or(bytes.len(), |limit| {
                    start.saturating_add(limit).min(bytes.len())
                });
                let selected = content_encoding.content(bytes[start..end].to_vec());
                let key = if selected.is_string() {
                    "text"
                } else {
                    "bytes"
                };
                Ok(json!({key:selected,"total_bytes":bytes.len(),"truncated":end<bytes.len()}))
            }
            Action::Plan {
                operations,
                content_encoding,
            } => {
                // A cancelled pending plan may have been signalled while the
                // bounded queue was full. It no longer consumes a plan slot.
                self.plans
                    .retain(|_, plan| !plan.cancellation.is_cancelled());
                {
                    let mut tokens = self.cancellations.lock().map_err(|_| {
                        anyhow::anyhow!("native backend cancellation lock poisoned")
                    })?;
                    tokens.retain(|id, _| self.plans.contains_key(id));
                }
                anyhow::ensure!(
                    self.plans.len() < MAX_PLANS,
                    "too many pending plans; commit or abort one first"
                );
                let plan = operations::prepare(operations)?;
                let cancellation = transaction::CancellationToken::default();
                let id = self.next_plan;
                self.next_plan = self
                    .next_plan
                    .checked_add(1)
                    .ok_or_else(|| anyhow::anyhow!("plan identifier exhausted"))?;
                let result = json!({"plan_id":id,"changes":content_encoding.changes(plan.snapshots()),"diff":plan.preview().diff});
                self.cancellations
                    .lock()
                    .map_err(|_| anyhow::anyhow!("native backend cancellation lock poisoned"))?
                    .insert(id, cancellation.clone());
                self.plans.insert(
                    id,
                    PendingPlan {
                        plan,
                        cancellation,
                        content_encoding,
                    },
                );
                Ok(result)
            }
            Action::Commit {
                plan_id,
                mut policy,
                backup,
            } => {
                let plan = self
                    .plans
                    .remove(&plan_id)
                    .ok_or_else(|| anyhow::anyhow!("unknown or consumed plan {plan_id}"))?;
                policy.backup = backup;
                let result = plan.plan.commit_controlled(
                    transaction::CommitOptions { backup },
                    &mut policy,
                    &plan.cancellation,
                );
                self.cancellations
                    .lock()
                    .map_err(|_| anyhow::anyhow!("native backend cancellation lock poisoned"))?
                    .remove(&plan_id);
                let result = result?;
                Ok(
                    json!({"written":result.written,"changes":plan.content_encoding.changes(result.changes),"journal":result.journal}),
                )
            }
            Action::Abort { plan_id } => {
                if let Some(plan) = self.plans.remove(&plan_id) {
                    plan.cancellation.cancel();
                    self.cancellations
                        .lock()
                        .map_err(|_| anyhow::anyhow!("native backend cancellation lock poisoned"))?
                        .remove(&plan_id);
                    Ok(json!({"aborted":true,"cancellation_requested":true}))
                } else if cancellation_requested {
                    // The commit ran ahead of this queued acknowledgement. Its
                    // own success/error response is the authoritative outcome.
                    Ok(json!({"cancellation_requested":true}))
                } else {
                    anyhow::bail!("unknown or consumed plan {plan_id}")
                }
            }
            Action::Recover {
                journal,
                check,
                policy,
            } => {
                let report = transaction::recover_authorized(&journal, check, &mut |paths| {
                    if !check {
                        for path in paths {
                            policy.authorize_path(path)?;
                        }
                    }
                    Ok(())
                })?;
                Ok(
                    json!({"files":report.files,"diff":report.diff,"journal":report.journal,"check":check}),
                )
            }
        }
    }
}

impl Policy {
    fn authorize_path(&self, path: &std::path::Path) -> anyhow::Result<()> {
        anyhow::ensure!(!self.read_only, "host policy is read-only");
        let normalized = crate::core::transaction::normalize(path)?;
        if let Some(roots) = &self.writable_roots {
            let mut permitted = false;
            for root in roots {
                anyhow::ensure!(root.is_absolute(), "writable roots must be absolute");
                let root = std::fs::canonicalize(root)?;
                if normalized.starts_with(&root) {
                    permitted = true;
                }
            }
            anyhow::ensure!(permitted, "host policy denies {}", path.display());
        }
        Ok(())
    }
}

impl transaction::Governance for Policy {
    fn authorize(&mut self, changes: &[transaction::FileChange]) -> anyhow::Result<()> {
        for change in changes {
            if change.before != change.after {
                self.authorize_path(&change.path)?;
                if self.backup && change.before.is_some() {
                    self.authorize_path(&crate::core::atomic::bak_path(&change.path))?;
                }
            }
        }
        Ok(())
    }

    fn checkpoint(
        &mut self,
        _step: usize,
        changes: &[transaction::FileChange],
    ) -> anyhow::Result<()> {
        self.authorize(changes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct CommitGate {
        begin_input: mpsc::Sender<()>,
        input_finished: mpsc::Receiver<()>,
    }

    impl transaction::Governance for CommitGate {
        fn checkpoint(&mut self, step: usize, _: &[transaction::FileChange]) -> anyhow::Result<()> {
            if step == 0 {
                self.begin_input.send(())?;
                self.input_finished.recv()?;
            }
            Ok(())
        }
    }

    #[test]
    fn input_reader_cancels_a_commit_while_the_persistence_worker_is_busy() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("file");
        std::fs::write(&path, b"before").unwrap();
        let plan = operations::prepare(vec![operations::Operation::Write {
            path: path.clone(),
            text: b"after".to_vec(),
            old: Some(b"before".to_vec()),
        }])
        .unwrap();
        let cancellation = transaction::CancellationToken::default();
        let cancellations = Cancellations::default();
        cancellations
            .lock()
            .unwrap()
            .insert(1, cancellation.clone());
        let (sender, receiver) = mpsc::sync_channel(MAX_QUEUED_REQUESTS);
        let output = Arc::new(Mutex::new(Vec::<u8>::new()));
        let (begin_input, wait_for_commit) = mpsc::channel();
        let (input_done, input_finished) = mpsc::channel();
        let reader = thread::spawn(move || {
            wait_for_commit.recv().unwrap();
            let status = read_requests(
                std::io::Cursor::new(
                    b"{\"id\":7,\"protocol\":1,\"action\":\"abort\",\"plan_id\":1}\n",
                ),
                &sender,
                &cancellations,
                &output,
            )
            .unwrap();
            input_done.send(()).unwrap();
            status
        });
        let mut governance = CommitGate {
            begin_input,
            input_finished,
        };
        let transaction::ControlError::Partial(error) = plan
            .commit_controlled(
                transaction::CommitOptions::default(),
                &mut governance,
                &cancellation,
            )
            .unwrap_err()
        else {
            panic!("active cancellation must roll back")
        };
        assert_eq!(reader.join().unwrap(), crate::output::exit::SUCCESS);
        let incoming = receiver.recv().unwrap();
        assert_eq!(incoming.id, 7);
        assert!(incoming.cancellation_requested);
        assert!(matches!(
            incoming.action.unwrap(),
            Action::Abort { plan_id: 1 }
        ));
        assert_eq!(std::fs::read(&path).unwrap(), b"before");
        assert_eq!(error.rollback, "completed");
        std::fs::remove_dir_all(error.journal).unwrap();
    }

    #[test]
    fn a_full_request_queue_cannot_block_the_abort_control_signal() {
        let cancellations = Cancellations::default();
        let token = transaction::CancellationToken::default();
        cancellations.lock().unwrap().insert(1, token.clone());
        let (sender, _receiver) = mpsc::sync_channel(MAX_QUEUED_REQUESTS);
        for id in 0..MAX_QUEUED_REQUESTS {
            sender
                .try_send(Incoming {
                    id: json!(id),
                    action: Ok(Action::Capabilities {}),
                    cancellation_requested: false,
                })
                .ok()
                .unwrap();
        }
        let output = Arc::new(Mutex::new(Vec::<u8>::new()));
        read_requests(
            std::io::Cursor::new(
                b"{\"id\":99,\"protocol\":1,\"action\":\"abort\",\"plan_id\":1}\n",
            ),
            &sender,
            &cancellations,
            &output,
        )
        .unwrap();
        assert!(token.is_cancelled());
        let reply: Value = serde_json::from_slice(&output.lock().unwrap()).unwrap();
        assert_eq!(reply["id"], 99);
        assert_eq!(reply["result"]["cancellation_requested"], true);
        assert!(reply["result"].get("aborted").is_none());
    }
}
