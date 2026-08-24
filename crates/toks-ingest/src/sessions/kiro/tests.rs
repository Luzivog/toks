use super::*;
use std::fs;
use std::io::Write;
use tempfile::TempDir;

mod cli_and_sqlite;
mod global_storage;
mod ide_sessions;
mod snapshot_discovery;

fn create_session_files(dir: &TempDir, stem: &str, json: &str, jsonl: &str) -> std::path::PathBuf {
    let json_path = dir.path().join(format!("{}.json", stem));
    let jsonl_path = dir.path().join(format!("{}.jsonl", stem));
    let mut f = std::fs::File::create(&json_path).unwrap();
    f.write_all(json.as_bytes()).unwrap();
    let mut f = std::fs::File::create(&jsonl_path).unwrap();
    f.write_all(jsonl.as_bytes()).unwrap();
    json_path
}

fn make_globalstorage_message(
    session_id: &str,
    dedup_key: &str,
    workspace: Option<&str>,
) -> UnifiedMessage {
    let mut message = UnifiedMessage::new_with_dedup(
        CLIENT_ID,
        "auto".to_string(),
        PROVIDER_ID,
        session_id.to_string(),
        1_770_983_426_000,
        TokenBreakdown {
            input: 100,
            output: 10,
            cache_read: 0,
            cache_write: 0,
            reasoning: 0,
        },
        0.0,
        Some(dedup_key.to_string()),
    );
    message.set_workspace(workspace.map(str::to_string), workspace.map(str::to_string));
    message
}

/// Build the Kiro IDE session layout on disk:
/// `<base>/.kiro/sessions/<workspace>/<sess_dir>/{session.json,messages.jsonl}`
/// and return the path to `session.json`.
fn create_ide_session_files(
    dir: &TempDir,
    workspace: &str,
    sess_dir: &str,
    session_json: &str,
    messages_jsonl: &str,
) -> std::path::PathBuf {
    let sess_path = dir
        .path()
        .join(".kiro/sessions")
        .join(workspace)
        .join(sess_dir);
    fs::create_dir_all(&sess_path).unwrap();
    let session_path = sess_path.join("session.json");
    fs::write(&session_path, session_json).unwrap();
    fs::write(sess_path.join("messages.jsonl"), messages_jsonl).unwrap();
    session_path
}
