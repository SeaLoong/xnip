//! NDJSON 输出（`--json`）。
//!
//! 事件类型：
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
    use super::{Event, emit};

    #[test]
    fn events_round_trip_as_independent_ndjson_records() {
        let path = "文件\nwith \"quotes\".txt";
        let mut bytes = Vec::new();
        for event in [
            Event::Start { command: "apply" },
            Event::Op {
                file: path,
                op: "replace",
            },
            Event::Done {
                affected_files: vec![path.into()],
            },
            Event::Error {
                kind: "phase1",
                message: "bad\ninput".into(),
            },
        ] {
            emit(&mut bytes, &event).unwrap();
        }
        let text = String::from_utf8(bytes).unwrap();
        let records: Vec<serde_json::Value> = text
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(records.len(), 4);
        assert_eq!(records[0]["event"], "start");
        assert_eq!(records[1]["file"], path);
        assert_eq!(records[2]["affected_files"][0], path);
        assert_eq!(records[3]["message"], "bad\ninput");
        assert!(text.ends_with('\n'));
    }

    #[test]
    fn a_closed_output_pipe_is_an_error() {
        let mut output = [0_u8; 0];
        let error = emit(output.as_mut_slice(), &Event::Start { command: "apply" });
        assert!(error.is_err());
    }
}
