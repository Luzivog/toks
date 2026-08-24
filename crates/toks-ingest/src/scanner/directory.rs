use std::path::{Path, PathBuf};

use rayon::iter::{ParallelBridge, ParallelIterator};
use walkdir::WalkDir;

/// Scan one directory recursively for files matching a scanner pattern.
pub fn scan_directory(root: &str, pattern: &str) -> Vec<PathBuf> {
    if !Path::new(root).exists() {
        return Vec::new();
    }

    let mut paths: Vec<PathBuf> = WalkDir::new(root)
        .into_iter()
        .par_bridge()
        .filter_map(Result::ok)
        .filter(|entry| {
            let path = entry.path();
            let file_type = entry.file_type();
            let is_file = file_type.is_file() || (file_type.is_symlink() && path.is_file());
            if !is_file {
                return false;
            }

            let file_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            let is_in_archive_dir = path.components().any(|component| {
                component
                    .as_os_str()
                    .to_string_lossy()
                    .eq_ignore_ascii_case("archive")
            });

            matches_pattern(path, file_name, pattern, is_in_archive_dir)
        })
        .map(|entry| entry.path().to_path_buf())
        .collect();
    paths.sort_unstable();
    paths
}

fn matches_pattern(path: &Path, file_name: &str, pattern: &str, in_archive: bool) -> bool {
    match pattern {
        "*.json" => file_name.ends_with(".json"),
        "*.json|*.jsonl" => file_name.ends_with(".json") || file_name.ends_with(".jsonl"),
        "*.jsonl" => file_name.ends_with(".jsonl"),
        "prime-agent-session" => {
            file_name.ends_with(".jsonl") && file_name != "rlm-subagents.jsonl"
        }
        "*.ndjson" => file_name.ends_with(".ndjson"),
        "*.log" => file_name.ends_with(".log"),
        "codebuddy-extension-log" => {
            file_name.ends_with(".log")
                && path.components().any(|component| {
                    component
                        .as_os_str()
                        .to_string_lossy()
                        .eq_ignore_ascii_case("Tencent-Cloud.coding-copilot")
                })
        }
        "*.jsonl*" => {
            file_name.ends_with(".jsonl")
                || file_name.contains(".jsonl.deleted.")
                || file_name.contains(".jsonl.reset.")
        }
        "*.csv" => file_name.ends_with(".csv"),
        "usage*.csv" => matches_usage_file(file_name, ".csv", in_archive),
        "usage*.json" => matches_usage_file(file_name, ".json", in_archive),
        "session-*.json" => file_name.starts_with("session-") && file_name.ends_with(".json"),
        "session_*.json" => file_name.starts_with("session_") && file_name.ends_with(".json"),
        "T-*.json" => file_name.starts_with("T-") && file_name.ends_with(".json"),
        "*.settings.json" => file_name.ends_with(".settings.json"),
        "kiro-globalstorage" => {
            file_name.ends_with(".chat")
                || file_name.ends_with(".json")
                || path.extension().is_none()
        }
        "kiro-ide-session" => {
            file_name == "session.json"
                && path
                    .parent()
                    .and_then(Path::file_name)
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("sess_"))
        }
        "sessions.json" => file_name == "sessions.json",
        "wire.jsonl" => file_name == "wire.jsonl",
        "updates.jsonl" => file_name == "updates.jsonl",
        "unified.jsonl" => file_name == "unified.jsonl",
        "events.jsonl" => file_name == "events.jsonl",
        "ui_messages.json" => file_name == "ui_messages.json",
        "cline-cli-messages" => file_name.ends_with(".messages.json"),
        "session-usage.json" => file_name == "session-usage.json",
        "chat-messages.json" => file_name == "chat-messages.json",
        "workbuddy.db" => file_name == "workbuddy.db",
        "sessions.db" => file_name == "sessions.db",
        "state.db" => file_name == "state.db",
        "threads.db" => file_name == "threads.db",
        "*.db" => file_name.ends_with(".db"),
        _ => false,
    }
}

fn matches_usage_file(file_name: &str, suffix: &str, in_archive: bool) -> bool {
    if in_archive {
        return false;
    }
    if file_name == format!("usage{suffix}") {
        return true;
    }
    file_name.starts_with("usage.")
        && file_name.ends_with(suffix)
        && !file_name.starts_with("usage.backup")
}
