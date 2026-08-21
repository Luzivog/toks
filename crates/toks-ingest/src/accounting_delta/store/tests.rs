use std::collections::BTreeMap;
use std::fs;

use tempfile::TempDir;

use super::{CheckpointStore, SampleDigest, StoredCheckpoint, DATABASE_FILE, LEGACY_FILE};
use crate::accounting_delta::types::{SourceKey, SourceKind};

fn checkpoint(kind: SourceKind, offset: u64) -> StoredCheckpoint {
    StoredCheckpoint {
        kind,
        parser_version: 7,
        committed_offset: offset,
        source_size: offset,
        modified_ns: 123,
        content_hash: [offset as u8; 32],
        prefix_samples: vec![SampleDigest {
            offset: 0,
            len: offset,
            hash: [3; 32],
        }],
        codex_state: (kind == SourceKind::Codex).then(Default::default),
    }
}

fn write_legacy(
    directory: &TempDir,
    schema_version: u32,
    sources: BTreeMap<String, StoredCheckpoint>,
) {
    let json = serde_json::json!({
        "schema_version": schema_version,
        "sources": sources,
        "rotation_cursor": "opaque-b"
    });
    fs::write(
        directory.path().join(LEGACY_FILE),
        serde_json::to_vec(&json).unwrap(),
    )
    .unwrap();
}

#[test]
fn legacy_json_import_is_committed_validated_and_reopenable() {
    let directory = TempDir::new().unwrap();
    let mut sources = BTreeMap::new();
    sources.insert("opaque-a".to_string(), checkpoint(SourceKind::Codex, 41));
    sources.insert("opaque-b".to_string(), checkpoint(SourceKind::Claude, 82));
    write_legacy(&directory, 1, sources);

    let store = CheckpointStore::open(directory.path().to_path_buf()).unwrap();
    assert_eq!(
        store
            .get(&SourceKey::new("opaque-a".to_string()))
            .unwrap()
            .unwrap()
            .committed_offset,
        41
    );
    assert_eq!(
        store.rotation_cursor().unwrap().as_deref(),
        Some("opaque-b")
    );
    assert!(!directory.path().join(LEGACY_FILE).exists());
    assert!(directory
        .path()
        .join(format!("{LEGACY_FILE}.migrated"))
        .exists());
    drop(store);

    let reopened = CheckpointStore::open(directory.path().to_path_buf()).unwrap();
    assert_eq!(
        reopened
            .get(&SourceKey::new("opaque-b".to_string()))
            .unwrap()
            .unwrap()
            .committed_offset,
        82
    );
}

#[test]
fn identical_commit_writes_nothing_and_update_is_row_local() {
    let directory = TempDir::new().unwrap();
    let mut store = CheckpointStore::open(directory.path().to_path_buf()).unwrap();
    let first_key = SourceKey::new("opaque-a".to_string());
    let second_key = SourceKey::new("opaque-b".to_string());
    let first = checkpoint(SourceKind::Codex, 41);
    let second = checkpoint(SourceKind::Claude, 82);
    store
        .commit([(&first_key, &first), (&second_key, &second)].into_iter())
        .unwrap();

    let before_noop = store.connection.total_changes();
    store
        .commit([(&first_key, &first), (&second_key, &second)].into_iter())
        .unwrap();
    assert_eq!(store.connection.total_changes(), before_noop);

    let updated = checkpoint(SourceKind::Codex, 42);
    store.commit([(&first_key, &updated)].into_iter()).unwrap();
    assert_eq!(store.connection.total_changes(), before_noop + 1);
    assert_eq!(
        store.get(&second_key).unwrap().unwrap().committed_offset,
        82
    );
}

#[test]
fn malformed_and_newer_legacy_schemas_are_rejected_without_renaming() {
    let malformed = TempDir::new().unwrap();
    fs::write(malformed.path().join(LEGACY_FILE), b"not-json").unwrap();
    assert!(CheckpointStore::open(malformed.path().to_path_buf()).is_err());
    assert!(malformed.path().join(LEGACY_FILE).exists());

    let newer = TempDir::new().unwrap();
    write_legacy(&newer, 2, BTreeMap::new());
    let error = match CheckpointStore::open(newer.path().to_path_buf()) {
        Ok(_) => panic!("newer schema unexpectedly opened"),
        Err(error) => error,
    };
    assert_eq!(error, "unsupported accounting checkpoint schema");
    assert!(newer.path().join(LEGACY_FILE).exists());
}

#[test]
fn newer_sqlite_schema_is_rejected() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join(DATABASE_FILE);
    let connection = rusqlite::Connection::open(path).unwrap();
    connection.execute_batch("PRAGMA user_version = 2").unwrap();
    drop(connection);

    let error = match CheckpointStore::open(directory.path().to_path_buf()) {
        Ok(_) => panic!("newer schema unexpectedly opened"),
        Err(error) => error,
    };
    assert_eq!(error, "unsupported accounting checkpoint schema");
}

#[test]
fn sqlite_checkpoint_contains_no_raw_source_path() {
    let directory = TempDir::new().unwrap();
    let mut store = CheckpointStore::open(directory.path().to_path_buf()).unwrap();
    let key = SourceKey::new("opaque-key".to_string());
    let value = checkpoint(SourceKind::Codex, 41);
    store.commit([(&key, &value)].into_iter()).unwrap();
    drop(store);

    let bytes = fs::read(directory.path().join(DATABASE_FILE)).unwrap();
    assert!(!bytes
        .windows(b"/private/session.jsonl".len())
        .any(|window| { window == b"/private/session.jsonl" }));
}
