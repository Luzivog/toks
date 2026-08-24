use super::*;
use std::io::Write;
use tempfile::NamedTempFile;

mod code;
mod wire;

fn create_test_file(content: &str) -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(content.as_bytes()).unwrap();
    file.flush().unwrap();
    file
}

fn create_kimi_code_test_file(content: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    // Build a fake kimi-code path so extract_session_id_from_kimi_code_path works:
    //   .../.kimi-code/sessions/ws/session-uuid/agents/main/wire.jsonl
    let fake_path = dir
        .path()
        .join(".kimi-code")
        .join("sessions")
        .join("test-ws")
        .join("sess-abc-123")
        .join("agents")
        .join("main")
        .join("wire.jsonl");
    std::fs::create_dir_all(fake_path.parent().unwrap()).unwrap();
    std::fs::write(&fake_path, content).unwrap();
    (dir, fake_path)
}
