use super::*;

mod reconciliation;
mod selector;
mod unified_log;
mod updates;

/// `updates_jsonl` is taken as bytes so fixtures can contain sequences a
/// `&str` cannot hold (undecodable bytes, a UTF-8 BOM); `&str` and `&String`
/// still pass through unchanged.
fn write_fixture(
    updates_jsonl: impl AsRef<[u8]>,
    summary_json: Option<&str>,
    signals_json: Option<&str>,
) -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::TempDir::new().unwrap();
    let session_dir = temp
        .path()
        .join(".grok")
        .join("sessions")
        .join("%2Ftmp%2Fproject")
        .join("session-1");
    std::fs::create_dir_all(&session_dir).unwrap();
    let updates_path = session_dir.join("updates.jsonl");
    std::fs::write(&updates_path, updates_jsonl.as_ref()).unwrap();
    if let Some(summary_json) = summary_json {
        std::fs::write(session_dir.join("summary.json"), summary_json).unwrap();
    }
    if let Some(signals_json) = signals_json {
        std::fs::write(session_dir.join("signals.json"), signals_json).unwrap();
    }
    (temp, updates_path)
}

fn usage_line(event_id: &str, timestamp_ms: i64, input: i64, output: i64) -> String {
    format!(
        r#"{{"method":"session/update","params":{{"sessionId":"session-1","update":{{"sessionUpdate":"turn_completed","usage":{{"inputTokens":{input},"outputTokens":{output},"totalTokens":{}}}}},"_meta":{{"eventId":"{event_id}","agentTimestampMs":{timestamp_ms}}}}}}}"#,
        input + output
    )
}

fn write_unified_fixture(unified_jsonl: &str) -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::TempDir::new().unwrap();
    let logs_dir = temp.path().join(".grok/logs");
    std::fs::create_dir_all(&logs_dir).unwrap();
    let path = logs_dir.join("unified.jsonl");
    std::fs::write(&path, unified_jsonl).unwrap();
    (temp, path)
}

fn test_message(session_id: &str, dedup_key: &str) -> UnifiedMessage {
    UnifiedMessage::new_with_dedup(
        CLIENT_ID,
        "grok-build",
        PROVIDER_ID,
        session_id,
        1_700_000_000_000,
        TokenBreakdown::default(),
        0.0,
        Some(dedup_key.to_string()),
    )
}
