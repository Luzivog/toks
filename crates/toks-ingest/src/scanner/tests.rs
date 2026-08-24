use super::*;
use crate::paths::json_path_literal;
use crate::paths::test_env::EnvGuard;
use serial_test::serial;
use std::fs::{self, File};
use std::io::Write;
use tempfile::TempDir;

mod assistant_clients;
mod client_types;
mod cline_family;
mod codebuff;
mod codex_kimi;
mod crush;
mod directory_patterns;
mod gjc;
mod grok_jcode;
mod ide_clients;
mod opencode_micode;
mod orchestration;
mod pi_family;
mod scanner_settings;
mod settings_profiles;

fn scan_without_extra_dirs(home_dir: &str, clients: &[String]) -> ScanResult {
    let mut extra = EnvGuard::capture(&["TOKSCOPE_EXTRA_DIRS", "TOKSCOPE_HEADLESS_DIR"]);
    extra.remove("TOKSCOPE_EXTRA_DIRS");
    extra.remove("TOKSCOPE_HEADLESS_DIR");
    scan_all_clients(home_dir, clients)
}

fn restore_env(var: &str, previous: Option<String>) {
    match previous {
        Some(value) => unsafe { std::env::set_var(var, value) },
        None => unsafe { std::env::remove_var(var) },
    }
}

fn restore_current_dir(previous: &Path) {
    std::env::set_current_dir(previous).unwrap();
}

struct CurrentDirGuard(PathBuf);

impl CurrentDirGuard {
    fn set(path: &Path) -> Self {
        let previous = std::env::current_dir().unwrap();
        std::env::set_current_dir(path).unwrap();
        Self(previous)
    }
}

impl Drop for CurrentDirGuard {
    fn drop(&mut self) {
        std::env::set_current_dir(&self.0).unwrap();
    }
}

fn setup_mock_copilot_dir(home: &Path) {
    let sessions_dir = home.join(".copilot/otel");
    fs::create_dir_all(&sessions_dir).unwrap();
    let file_path = sessions_dir.join("copilot.jsonl");
    let mut file = File::create(file_path).unwrap();
    writeln!(file, "{{\"type\":\"span\",\"name\":\"chat gpt-5.4-mini\"}}").unwrap();
}

fn setup_mock_opencode_dir(base: &std::path::Path) {
    let opencode_path = base.join(".local/share/opencode/storage/message/proj1");
    fs::create_dir_all(&opencode_path).unwrap();
    let mut file = File::create(opencode_path.join("msg_001.json")).unwrap();
    file.write_all(b"{}").unwrap();
}

fn setup_mock_claude_dir(base: &std::path::Path) {
    let claude_path = base.join(".claude/projects/myproject");
    fs::create_dir_all(&claude_path).unwrap();
    let mut file = File::create(claude_path.join("conversation.jsonl")).unwrap();
    file.write_all(b"").unwrap();
}

fn setup_mock_claude_transcripts_dir(base: &std::path::Path) -> PathBuf {
    let transcript_path = base.join(".claude/transcripts");
    fs::create_dir_all(&transcript_path).unwrap();
    let file_path = transcript_path.join("ses_123456789012345678901234567.jsonl");
    let mut file = File::create(&file_path).unwrap();
    file.write_all(b"").unwrap();
    file_path
}

fn setup_mock_codex_dir(base: &std::path::Path) {
    let codex_path = base.join(".codex/sessions");
    fs::create_dir_all(&codex_path).unwrap();
    let mut file = File::create(codex_path.join("session.jsonl")).unwrap();
    file.write_all(b"").unwrap();
}

fn setup_mock_codex_archived_dir(base: &std::path::Path) {
    let archived_path = base.join(".codex/archived_sessions");
    fs::create_dir_all(&archived_path).unwrap();
    let mut file = File::create(archived_path.join("archived.jsonl")).unwrap();
    file.write_all(b"").unwrap();
}

fn setup_mock_gemini_dir(base: &std::path::Path) {
    let gemini_path = base.join(".gemini/tmp/123/chats");
    fs::create_dir_all(&gemini_path).unwrap();
    let mut file = File::create(gemini_path.join("session-abc.json")).unwrap();
    file.write_all(b"{}").unwrap();
}

fn setup_mock_pi_dir(base: &std::path::Path) {
    let pi_path = base.join(".pi/agent/sessions/--test--");
    fs::create_dir_all(&pi_path).unwrap();
    let mut file = File::create(pi_path.join("1733011200000_pi_ses_001.jsonl")).unwrap();
    file.write_all(b"{}").unwrap();
}

fn setup_mock_kimchi_dir(base: &std::path::Path) {
    let kimchi_path = base.join(".config/kimchi/harness/sessions/--test--");
    fs::create_dir_all(&kimchi_path).unwrap();
    let mut file =
        File::create(kimchi_path.join("2026-08-01T00-00-00Z_kimchi_ses_001.jsonl")).unwrap();
    file.write_all(b"{}").unwrap();
}

fn setup_mock_kiro_dir(base: &std::path::Path) {
    let kiro_path = base.join(".kiro/sessions/cli");
    fs::create_dir_all(&kiro_path).unwrap();
    File::create(kiro_path.join("session-001.json")).unwrap();
}

fn setup_mock_kiro_global_storage_dir(base: &std::path::Path) {
    let root = base.join("Library/Application Support/Kiro/User/globalStorage/kiro.kiroagent");
    let workspace = root.join("workspace-a");
    fs::create_dir_all(&workspace).unwrap();
    File::create(workspace.join("execution.chat")).unwrap();
    File::create(workspace.join("session.json")).unwrap();
    File::create(workspace.join("execution")).unwrap();
}

fn setup_mock_senpi_dir(base: &std::path::Path) {
    let senpi_path = base.join(".senpi/agent/sessions/--Users-someone-project--");
    fs::create_dir_all(&senpi_path).unwrap();
    let mut file = File::create(
        senpi_path.join("2026-07-29T15-19-53-436Z_019fae75-f35c-7b20-8d6f-e6dea8f7d9f5.jsonl"),
    )
    .unwrap();
    file.write_all(b"{}").unwrap();
}

fn setup_mock_omp_dir(base: &std::path::Path) {
    let omp_path = base.join(".omp/agent/sessions/--omp-test--");
    fs::create_dir_all(&omp_path).unwrap();
    let mut file = File::create(omp_path.join("2026-04-06T03-04-28Z_omp_ses_001.jsonl")).unwrap();
    file.write_all(b"{}").unwrap();
}

fn setup_mock_zed_xdg_db(base: &std::path::Path) -> PathBuf {
    let zed_db = base.join(".local/share/zed/threads/threads.db");
    fs::create_dir_all(zed_db.parent().unwrap()).unwrap();
    File::create(&zed_db).unwrap();
    zed_db
}

#[cfg(target_os = "macos")]
fn setup_mock_zed_macos_db(base: &std::path::Path) -> PathBuf {
    let zed_db = base.join("Library/Application Support/Zed/threads/threads.db");
    fs::create_dir_all(zed_db.parent().unwrap()).unwrap();
    File::create(&zed_db).unwrap();
    zed_db
}

fn setup_mock_kimi_dir(base: &std::path::Path) {
    let kimi_session = base.join(".kimi/sessions/group1/session-uuid-1");
    fs::create_dir_all(&kimi_session).unwrap();
    let mut file = File::create(kimi_session.join("wire.jsonl")).unwrap();
    file.write_all(b"{\"type\": \"metadata\", \"protocol_version\": \"1.3\"}\n")
        .unwrap();
}

/// Kimi Code lays sessions out as
/// `<root>/sessions/WORKSPACE/SESSION/agents/AGENT/wire.jsonl`, where
/// `<root>` is `~/.kimi-code` or whatever KIMI_CODE_HOME points at.
fn setup_mock_kimi_code_dir(root: &std::path::Path) -> PathBuf {
    let agent_dir = root.join("sessions/workspace-1/session-uuid-1/agents/main");
    fs::create_dir_all(&agent_dir).unwrap();
    let wire = agent_dir.join("wire.jsonl");
    let mut file = File::create(&wire).unwrap();
    file.write_all(b"{\"type\": \"metadata\", \"protocol_version\": \"1.3\"}\n")
        .unwrap();
    wire
}

fn setup_mock_grok_dir(base: &std::path::Path) {
    let grok_session = base.join(".grok/sessions/%2Ftmp%2Fproject/session-uuid-1");
    fs::create_dir_all(&grok_session).unwrap();
    let mut file = File::create(grok_session.join("updates.jsonl")).unwrap();
    file.write_all(b"{\"method\":\"session/update\"}\n")
        .unwrap();
}

fn setup_mock_jcode_dir(base: &std::path::Path) {
    let jcode_sessions = base.join(".jcode/sessions");
    fs::create_dir_all(&jcode_sessions).unwrap();
    File::create(jcode_sessions.join("session_fixture.json")).unwrap();
    File::create(jcode_sessions.join("not-a-session.json")).unwrap();
}

fn setup_mock_openclaw_dir(base: &std::path::Path) {
    // Mirror real OpenClaw layout: ~/.openclaw/agents/<agentId>/sessions/*.jsonl
    let openclaw_sessions = base.join(".openclaw/agents/main/sessions");
    fs::create_dir_all(&openclaw_sessions).unwrap();

    let mut transcript = File::create(openclaw_sessions.join("session-abc.jsonl")).unwrap();
    transcript.write_all(b"{}").unwrap();

    let mut archived_deleted =
        File::create(openclaw_sessions.join("session-deleted.jsonl.deleted.123")).unwrap();
    archived_deleted.write_all(b"{}").unwrap();

    let mut archived_reset =
        File::create(openclaw_sessions.join("session-reset.jsonl.reset.456")).unwrap();
    archived_reset.write_all(b"{}").unwrap();

    // Even if an index exists, we should count JSONL transcripts (not sessions.json only)
    let mut index = File::create(openclaw_sessions.join("sessions.json")).unwrap();
    index.write_all(b"{}").unwrap();
}

fn setup_mock_roocode_dir(base: &std::path::Path) {
    let local =
        base.join(".config/Code/User/globalStorage/rooveterinaryinc.roo-cline/tasks/task-local");
    let server = base.join(
        ".vscode-server/data/User/globalStorage/rooveterinaryinc.roo-cline/tasks/task-server",
    );
    fs::create_dir_all(&local).unwrap();
    fs::create_dir_all(&server).unwrap();
    File::create(local.join("ui_messages.json")).unwrap();
    File::create(server.join("ui_messages.json")).unwrap();
}

fn setup_mock_kilocode_dir(base: &std::path::Path) {
    let local = base.join(".config/Code/User/globalStorage/kilocode.kilo-code/tasks/task-local");
    let server =
        base.join(".vscode-server/data/User/globalStorage/kilocode.kilo-code/tasks/task-server");
    fs::create_dir_all(&local).unwrap();
    fs::create_dir_all(&server).unwrap();
    File::create(local.join("ui_messages.json")).unwrap();
    File::create(server.join("ui_messages.json")).unwrap();
}

fn setup_mock_cline_dir(base: &std::path::Path) {
    let local =
        base.join(".config/Code/User/globalStorage/saoudrizwan.claude-dev/tasks/task-local");
    let macos = base.join(
        "Library/Application Support/Code/User/globalStorage/saoudrizwan.claude-dev/tasks/task-macos",
    );
    let windows = base
        .join("AppData/Roaming/Code/User/globalStorage/saoudrizwan.claude-dev/tasks/task-windows");
    let server = base
        .join(".vscode-server/data/User/globalStorage/saoudrizwan.claude-dev/tasks/task-server");
    fs::create_dir_all(&local).unwrap();
    fs::create_dir_all(&macos).unwrap();
    fs::create_dir_all(&windows).unwrap();
    fs::create_dir_all(&server).unwrap();
    File::create(local.join("ui_messages.json")).unwrap();
    File::create(macos.join("ui_messages.json")).unwrap();
    File::create(windows.join("ui_messages.json")).unwrap();
    File::create(server.join("ui_messages.json")).unwrap();
}

fn setup_mock_cline_cli_dir(data_dir: &std::path::Path) {
    setup_mock_cline_cli_session_root(&data_dir.join("sessions"));
}

fn setup_mock_cline_cli_session_root(sessions_root: &std::path::Path) {
    let sessions = sessions_root.join("cli-session");
    fs::create_dir_all(&sessions).unwrap();
    File::create(sessions.join("cli-session.messages.json")).unwrap();
}

fn setup_mock_crush_registry(registry_path: &Path, projects_json: &str) {
    fs::create_dir_all(registry_path.parent().unwrap()).unwrap();
    fs::write(registry_path, projects_json).unwrap();
}
