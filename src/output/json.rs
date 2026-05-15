//! NDJSON 输出（`--json`）。
//!
//! 事件类型（PLAN §6.7.5 简化）：
//!
//! - `start`：批次开始（含命令名、参数概要）
//! - `op`：单个 op 完成（含文件路径、op 类型、变更概要）
//! - `done`：批次成功结束
//! - `error`：错误（含 `kind` / `message`）
//!
//! 每行一个 JSON 对象，UTF-8，`\n` 分隔。设计上让 agent 按行流式消费。

use std::io::Write;

use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "event")]
pub enum Event<'a> {
    Start { command: &'a str },
    Op { file: &'a str, op: &'a str },
    Done { affected_files: Vec<String> },
    Error { kind: &'a str, message: String },
}

/// 把事件写为 NDJSON 一行。
///
/// # Errors
/// IO 失败或序列化失败。
pub fn emit<W: Write>(mut out: W, event: &Event<'_>) -> std::io::Result<()> {
    let line = serde_json::to_string(event)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    out.write_all(line.as_bytes())?;
    out.write_all(b"\n")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn emit_one(e: &Event<'_>) -> String {
        let mut buf = Vec::new();
        emit(&mut buf, e).unwrap();
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn start_event_format() {
        let s = emit_one(&Event::Start { command: "apply" });
        assert!(s.contains("\"event\":\"start\""));
        assert!(s.contains("\"command\":\"apply\""));
        assert!(s.ends_with('\n'));
    }

    #[test]
    fn op_event_format() {
        let s = emit_one(&Event::Op {
            file: "a.txt",
            op: "replace",
        });
        assert!(s.contains("\"event\":\"op\""));
        assert!(s.contains("\"file\":\"a.txt\""));
        assert!(s.contains("\"op\":\"replace\""));
    }

    #[test]
    fn done_event_lists_files() {
        let s = emit_one(&Event::Done {
            affected_files: vec!["a.txt".into(), "b.txt".into()],
        });
        assert!(s.contains("\"event\":\"done\""));
        assert!(s.contains("a.txt"));
        assert!(s.contains("b.txt"));
    }

    #[test]
    fn error_event_format() {
        let s = emit_one(&Event::Error {
            kind: "phase1",
            message: "bad locator".into(),
        });
        assert!(s.contains("\"event\":\"error\""));
        assert!(s.contains("\"kind\":\"phase1\""));
        assert!(s.contains("\"message\":\"bad locator\""));
    }
}
