use super::*;
use rusqlite::{params, Connection};
use serde_json::json;
use std::io::Write;
use tempfile::TempDir;

mod jsonl;
mod sqlite_cache;
mod sqlite_usage;

fn write_session(dir: &TempDir, slug: &str, session: &str, jsonl: &str) -> std::path::PathBuf {
    let project_dir = dir.path().join("projects").join(slug);
    std::fs::create_dir_all(&project_dir).unwrap();
    let path = project_dir.join(format!("{session}.jsonl"));
    let mut file = std::fs::File::create(&path).unwrap();
    file.write_all(jsonl.as_bytes()).unwrap();
    path
}

fn create_zcode_sqlite_db(dir: &TempDir) -> std::path::PathBuf {
    let db_path = dir.path().join("db.sqlite");
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE model_usage (
            id TEXT PRIMARY KEY,
            session_id TEXT,
            turn_id TEXT,
            model_id TEXT,
            started_at INTEGER,
            completed_at INTEGER,
            duration_ms INTEGER,
            input_tokens INTEGER,
            output_tokens INTEGER,
            reasoning_tokens INTEGER,
            cache_read_input_tokens INTEGER,
            cache_creation_input_tokens INTEGER,
            computed_total_tokens INTEGER,
            agent TEXT,
            mode TEXT
        );
        CREATE TABLE session (
            id TEXT PRIMARY KEY,
            directory TEXT,
            path TEXT
        );
        "#,
    )
    .unwrap();
    db_path
}
