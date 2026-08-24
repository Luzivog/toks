use super::*;

#[test]
fn test_scan_directory_json_pattern() {
    let dir = TempDir::new().unwrap();
    let path = dir.path();

    // Create test files
    File::create(path.join("test1.json")).unwrap();
    File::create(path.join("test2.json")).unwrap();
    File::create(path.join("data.txt")).unwrap();
    File::create(path.join("other.jsonl")).unwrap();

    let json_files = scan_directory(path.to_str().unwrap(), "*.json");
    assert_eq!(json_files.len(), 2);
    assert!(json_files.iter().all(|p| p.extension().unwrap() == "json"));
}

#[test]
fn test_scan_directory_json_or_jsonl_pattern() {
    let dir = TempDir::new().unwrap();
    let path = dir.path();

    File::create(path.join("session.json")).unwrap();
    File::create(path.join("session.jsonl")).unwrap();
    File::create(path.join("session.txt")).unwrap();

    let session_files = scan_directory(path.to_str().unwrap(), "*.json|*.jsonl");
    assert_eq!(session_files.len(), 2);
    assert_eq!(
        session_files
            .iter()
            .map(|path| path.file_name().unwrap().to_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["session.json", "session.jsonl"]
    );
}

#[test]
fn test_scan_directory_jsonl_pattern() {
    let dir = TempDir::new().unwrap();
    let path = dir.path();

    File::create(path.join("session.jsonl")).unwrap();
    File::create(path.join("log.jsonl")).unwrap();
    File::create(path.join("data.json")).unwrap();

    let jsonl_files = scan_directory(path.to_str().unwrap(), "*.jsonl");
    assert_eq!(jsonl_files.len(), 2);
    assert!(jsonl_files
        .iter()
        .all(|p| p.extension().unwrap() == "jsonl"));
}

#[test]
fn test_scan_directory_log_pattern() {
    let dir = TempDir::new().unwrap();
    let path = dir.path();

    File::create(path.join("ide.log")).unwrap();
    File::create(path.join("vscode.log")).unwrap();
    File::create(path.join("session.jsonl")).unwrap();

    let log_files = scan_directory(path.to_str().unwrap(), "*.log");
    assert_eq!(log_files.len(), 2);
    assert!(log_files.iter().all(|p| p.extension().unwrap() == "log"));
}

#[test]
fn test_scan_directory_workbuddy_db_pattern() {
    let dir = TempDir::new().unwrap();
    let path = dir.path();

    File::create(path.join("workbuddy.db")).unwrap();
    File::create(path.join("workbuddy.db-wal")).unwrap();
    File::create(path.join("workbuddy.db-shm")).unwrap();

    let db_files = scan_directory(path.to_str().unwrap(), "workbuddy.db");

    assert_eq!(db_files, vec![path.join("workbuddy.db")]);
}

#[test]
fn test_scan_directory_updates_jsonl_pattern() {
    let dir = TempDir::new().unwrap();
    let path = dir.path();
    let session_dir = path.join("workspace/session-1");
    fs::create_dir_all(&session_dir).unwrap();

    File::create(session_dir.join("updates.jsonl")).unwrap();
    File::create(session_dir.join("events.jsonl")).unwrap();
    File::create(session_dir.join("updates.json")).unwrap();

    let updates_files = scan_directory(path.to_str().unwrap(), "updates.jsonl");
    assert_eq!(updates_files.len(), 1);
    assert!(updates_files[0].ends_with("updates.jsonl"));
}

#[test]
fn test_scan_directory_session_pattern() {
    let dir = TempDir::new().unwrap();
    let path = dir.path();

    File::create(path.join("session-001.json")).unwrap();
    File::create(path.join("session-abc.json")).unwrap();
    File::create(path.join("other.json")).unwrap();
    File::create(path.join("session.json")).unwrap(); // Shouldn't match

    let session_files = scan_directory(path.to_str().unwrap(), "session-*.json");
    assert_eq!(session_files.len(), 2);
    assert!(session_files.iter().all(|p| {
        let name = p.file_name().unwrap().to_str().unwrap();
        name.starts_with("session-") && name.ends_with(".json")
    }));
}

#[test]
fn test_scan_directory_ui_messages_pattern() {
    let dir = TempDir::new().unwrap();
    let path = dir.path();

    let tasks = path.join("tasks");
    fs::create_dir_all(tasks.join("task-a")).unwrap();
    fs::create_dir_all(tasks.join("task-b")).unwrap();
    fs::create_dir_all(tasks.join("task-c")).unwrap();

    File::create(tasks.join("task-a").join("ui_messages.json")).unwrap();
    File::create(tasks.join("task-b").join("ui_messages.json")).unwrap();
    File::create(tasks.join("task-c").join("api_conversation_history.json")).unwrap();

    let files = scan_directory(path.to_str().unwrap(), "ui_messages.json");
    assert_eq!(files.len(), 2);
    assert!(files.iter().all(|p| {
        p.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            == "ui_messages.json"
    }));
}

#[test]
fn test_scan_directory_cline_cli_messages_pattern() {
    let dir = TempDir::new().unwrap();
    let path = dir.path();

    File::create(path.join("session.messages.json")).unwrap();
    File::create(path.join("session.json")).unwrap();
    File::create(path.join("session.messages.jsonl")).unwrap();

    let files = scan_directory(path.to_str().unwrap(), "cline-cli-messages");
    assert_eq!(files, vec![path.join("session.messages.json")]);
}

#[test]
fn test_scan_directory_nested() {
    let dir = TempDir::new().unwrap();
    let path = dir.path();

    // Create nested structure
    let sub1 = path.join("project1");
    let sub2 = path.join("project2");
    fs::create_dir_all(&sub1).unwrap();
    fs::create_dir_all(&sub2).unwrap();

    File::create(sub1.join("session.json")).unwrap();
    File::create(sub2.join("session.json")).unwrap();
    File::create(path.join("root.json")).unwrap();

    let files = scan_directory(path.to_str().unwrap(), "*.json");
    assert_eq!(files.len(), 3);
}

#[test]
fn test_scan_directory_csv_pattern() {
    let dir = TempDir::new().unwrap();
    let path = dir.path();

    File::create(path.join("usage.csv")).unwrap();
    File::create(path.join("data.csv")).unwrap();
    File::create(path.join("other.json")).unwrap();

    let csv_files = scan_directory(path.to_str().unwrap(), "*.csv");
    assert_eq!(csv_files.len(), 2);
    assert!(csv_files.iter().all(|p| p.extension().unwrap() == "csv"));
}

#[test]
fn test_scan_directory_usage_json_pattern() {
    let dir = TempDir::new().unwrap();
    let path = dir.path();
    let archive = path.join("archive");
    fs::create_dir_all(&archive).unwrap();

    File::create(path.join("usage.json")).unwrap();
    File::create(path.join("usage.account.json")).unwrap();
    File::create(path.join("usage.backup-20240601.json")).unwrap();
    File::create(path.join("other.json")).unwrap();
    File::create(archive.join("usage.json")).unwrap();

    let usage_files = scan_directory(path.to_str().unwrap(), "usage*.json");
    let names: Vec<_> = usage_files
        .iter()
        .map(|path| path.file_name().unwrap().to_str().unwrap())
        .collect();

    assert_eq!(names, vec!["usage.account.json", "usage.json"]);
}

#[test]
fn test_scan_directory_kiro_globalstorage_pattern() {
    let dir = TempDir::new().unwrap();
    let path = dir.path();

    let root = path.join("Library/Application Support/Kiro/User/globalStorage/kiro.kiroagent");
    let workspace = root.join("workspace-a");
    fs::create_dir_all(&workspace).unwrap();
    File::create(workspace.join("execution.chat")).unwrap();
    File::create(workspace.join("session.json")).unwrap();
    File::create(workspace.join("execution")).unwrap();
    File::create(workspace.join("index.sqlite")).unwrap();

    let files = scan_directory(root.to_str().unwrap(), "kiro-globalstorage");
    let names: Vec<_> = files
        .iter()
        .map(|path| path.file_name().unwrap().to_str().unwrap())
        .collect();

    assert_eq!(names, vec!["execution", "execution.chat", "session.json"]);
}

#[test]
fn test_scan_directory_kiro_ide_session_pattern() {
    let dir = TempDir::new().unwrap();
    let sessions_root = dir.path().join(".kiro/sessions");

    // IDE layout: <workspace>/sess_<uuid>/{session.json,messages.jsonl}.
    let sess_dir = sessions_root.join("workspace-a/sess_02f1c107");
    fs::create_dir_all(&sess_dir).unwrap();
    File::create(sess_dir.join("session.json")).unwrap();
    File::create(sess_dir.join("messages.jsonl")).unwrap();

    // CLI layout under the same tree must NOT be matched by this pattern
    // (it is scanned separately as *.json), and a stray session.json outside
    // a sess_* dir must be ignored.
    let cli_dir = sessions_root.join("cli");
    fs::create_dir_all(&cli_dir).unwrap();
    File::create(cli_dir.join("session-001.json")).unwrap();
    File::create(sessions_root.join("workspace-a/session.json")).unwrap();

    let files = scan_directory(sessions_root.to_str().unwrap(), "kiro-ide-session");
    let names: Vec<_> = files
        .iter()
        .map(|path| {
            path.parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap()
                .to_string()
        })
        .collect();

    // Exactly one match: the session.json inside sess_02f1c107.
    assert_eq!(files.len(), 1);
    assert_eq!(names, vec!["sess_02f1c107"]);
}

#[test]
fn test_scan_directory_nonexistent() {
    let files = scan_directory("/nonexistent/path/that/does/not/exist", "*.json");
    assert!(files.is_empty());
}

#[test]
fn test_scan_all_clients_discovers_zcode_v2_sqlite() {
    let dir = TempDir::new().unwrap();
    let db_dir = dir.path().join(".zcode/cli/db");
    fs::create_dir_all(&db_dir).unwrap();
    let db_path = db_dir.join("db.sqlite");
    File::create(&db_path).unwrap();

    let result = scan_all_clients_with_env_strategy(
        dir.path().to_str().unwrap(),
        &["zcode".to_string()],
        false,
    );

    assert_eq!(result.zcode_db.as_deref(), Some(db_path.as_path()));
}

#[test]
fn test_scan_all_clients_discovers_codebuddy_extension_logs() {
    let dir = TempDir::new().unwrap();
    let ide_dir = dir
        .path()
        .join("AppData")
        .join("Local")
        .join("CodeBuddyExtension")
        .join("Logs")
        .join("CodeBuddyIDE")
        .join("2026-07-01");
    let vscode_dir = dir
        .path()
        .join("AppData")
        .join("Local")
        .join("CodeBuddyExtension")
        .join("Logs")
        .join("VSCode")
        .join("2026-07-01");
    fs::create_dir_all(&ide_dir).unwrap();
    fs::create_dir_all(&vscode_dir).unwrap();
    let ide_log = ide_dir.join("ide.log");
    let vscode_log = vscode_dir.join("vscode.log");
    File::create(&ide_log).unwrap();
    File::create(&vscode_log).unwrap();

    let result = scan_all_clients_with_env_strategy(
        dir.path().to_str().unwrap(),
        &["codebuddy".to_string()],
        false,
    );

    let files = result.get(ClientId::CodeBuddy);
    assert_eq!(files.len(), 2);
    assert!(files.contains(&ide_log));
    assert!(files.contains(&vscode_log));
}

#[test]
fn test_scan_all_clients_discovers_workbuddy_project_jsonl() {
    let dir = TempDir::new().unwrap();
    let project_dir = dir.path().join(".workbuddy/projects/project-a");
    fs::create_dir_all(&project_dir).unwrap();
    let session = project_dir.join("session.jsonl");
    File::create(&session).unwrap();

    let result = scan_all_clients_with_env_strategy(
        dir.path().to_str().unwrap(),
        &["workbuddy".to_string()],
        false,
    );

    let files = result.get(ClientId::WorkBuddy);
    assert_eq!(files.as_slice(), std::slice::from_ref(&session));
}

#[test]
fn test_scan_directory_empty() {
    let dir = TempDir::new().unwrap();
    let files = scan_directory(dir.path().to_str().unwrap(), "*.json");
    assert!(files.is_empty());
}

#[test]
fn test_scan_directory_deterministic_order() {
    let dir = TempDir::new().unwrap();
    let path = dir.path();

    for name in ["zebra.jsonl", "alpha.jsonl", "middle.jsonl", "beta.jsonl"] {
        File::create(path.join(name)).unwrap();
    }

    let first = scan_directory(path.to_str().unwrap(), "*.jsonl");
    let second = scan_directory(path.to_str().unwrap(), "*.jsonl");
    let third = scan_directory(path.to_str().unwrap(), "*.jsonl");

    assert_eq!(first, second, "Repeated scans must return identical order");
    assert_eq!(second, third, "Repeated scans must return identical order");

    let names: Vec<_> = first
        .iter()
        .map(|p| p.file_name().unwrap().to_str().unwrap())
        .collect();
    assert_eq!(
        names,
        vec!["alpha.jsonl", "beta.jsonl", "middle.jsonl", "zebra.jsonl"],
        "Results must be lexically sorted"
    );
}
