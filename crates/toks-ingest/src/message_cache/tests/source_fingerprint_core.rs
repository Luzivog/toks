use super::*;

#[test]
fn test_codex_prefix_matches_appended_file() {
    let file = write_temp_file(b"line-1\nline-2\n");
    let fingerprint = SourceFingerprint::from_path(file.path()).unwrap();
    let incremental_cache =
        build_codex_incremental_cache(file.path(), fingerprint.size, CodexParseState::default())
            .unwrap();

    let mut reopened = file.reopen().unwrap();
    reopened.seek(SeekFrom::End(0)).unwrap();
    reopened.write_all(b"line-3\n").unwrap();
    reopened.flush().unwrap();

    assert!(codex_prefix_matches(file.path(), &incremental_cache,));
}

#[test]
fn test_codex_incremental_cache_reuses_full_hash() {
    let file = write_temp_file(b"line-1\nline-2\n");
    let fingerprint = SourceFingerprint::from_path(file.path()).unwrap();
    let full_hashes_before = full_hash_call_count();

    let incremental_cache = build_codex_incremental_cache_with_prefix_hash(
        file.path(),
        fingerprint.size,
        CodexParseState::default(),
        fingerprint.content_hash,
    )
    .unwrap();

    assert_eq!(
        full_hash_call_count(),
        full_hashes_before,
        "a supplied Codex fingerprint must avoid a second whole-file SHA-256"
    );
    assert_eq!(incremental_cache.prefix_hash, fingerprint.content_hash);
    assert!(incremental_cache.ends_with_newline);
}

#[test]
fn test_check_path_returns_unchanged_for_matching_metadata_and_samples() {
    let file = write_temp_file(&vec![b'a'; 32 * 1024]);
    let fingerprint = SourceFingerprint::from_path(file.path()).unwrap();
    let full_hashes_before = full_hash_call_count();

    let status = SourceFingerprint::check_path(file.path(), Some(&fingerprint)).unwrap();

    assert!(matches!(status, FingerprintStatus::Unchanged));
    assert_eq!(
        full_hash_call_count(),
        full_hashes_before,
        "an unchanged fingerprint must not compute a full SHA-256"
    );
}

#[test]
fn test_check_path_returns_changed_when_sample_changes_with_same_metadata() {
    let original = vec![b'a'; 32 * 1024];
    let file = write_temp_file(&original);
    let fingerprint = SourceFingerprint::from_path(file.path()).unwrap();
    let original_signature = metadata_signature(file.path()).unwrap();
    let original_modified = std::fs::metadata(file.path()).unwrap().modified().unwrap();

    let mut rewritten = original;
    rewritten[0] = b'z';
    std::fs::write(file.path(), rewritten).unwrap();
    File::options()
        .write(true)
        .open(file.path())
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(original_modified))
        .unwrap();
    assert_eq!(metadata_signature(file.path()).unwrap(), original_signature);
    let full_hashes_before = full_hash_call_count();

    let status = SourceFingerprint::check_path(file.path(), Some(&fingerprint)).unwrap();

    let FingerprintStatus::Changed(changed) = status else {
        panic!("changed sample must rebuild the full fingerprint");
    };
    assert_ne!(changed, fingerprint);
    assert_eq!(
        full_hash_call_count(),
        full_hashes_before + 1,
        "a changed sample must rebuild the full fingerprint"
    );
}

#[test]
fn test_generic_sources_skip_full_hash() {
    let original = vec![b'a'; 64 * 1024];
    let file = write_temp_file(&original);
    let fingerprint = SourceFingerprint::from_path(file.path()).unwrap();
    let original_signature = metadata_signature(file.path()).unwrap();
    let original_modified = std::fs::metadata(file.path()).unwrap().modified().unwrap();

    let mut rewritten = original;
    rewritten[0] = b'z';
    std::fs::write(file.path(), rewritten).unwrap();
    File::options()
        .write(true)
        .open(file.path())
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(original_modified))
        .unwrap();
    assert_eq!(metadata_signature(file.path()).unwrap(), original_signature);

    let full_hashes_before = full_hash_call_count();
    let status =
        SourceFingerprint::check_path_samples_only(file.path(), Some(&fingerprint)).unwrap();
    let FingerprintStatus::Changed(changed) = status else {
        panic!("changed sample must invalidate a generic source");
    };
    assert_eq!(
        full_hash_call_count(),
        full_hashes_before,
        "generic source fingerprints must not compute a whole-file SHA-256"
    );
    assert_eq!(changed.content_hash, [0_u8; 32]);

    let full_hashes_before = full_hash_call_count();
    let cold = SourceFingerprint::check_path_samples_only(file.path(), None).unwrap();
    let FingerprintStatus::Changed(cold) = cold else {
        panic!("an uncached generic source must build a fingerprint");
    };
    assert_eq!(full_hash_call_count(), full_hashes_before);
    assert_eq!(cold.content_hash, [0_u8; 32]);
}

#[test]
fn test_sqlite_fingerprint_skips_full_hash() {
    let file = write_temp_file(&vec![b'a'; 64 * 1024]);
    let full_hashes_before = full_hash_call_count();

    let fingerprint = SourceFingerprint::from_sqlite_path(file.path()).unwrap();

    assert_eq!(
        full_hash_call_count(),
        full_hashes_before,
        "a SQLite fingerprint must not compute a whole-file SHA-256"
    );
    assert_eq!(
        fingerprint.content_hash, [0_u8; 32],
        "a SQLite fingerprint stores a zero content_hash sentinel"
    );
    assert!(
        !fingerprint.sample_hashes.is_empty(),
        "samples still guard SQLite change detection"
    );
}

#[test]
fn test_sqlite_check_detects_change_without_full_hash() {
    let original = vec![b'a'; 64 * 1024];
    let file = write_temp_file(&original);
    let fingerprint = SourceFingerprint::from_sqlite_path(file.path()).unwrap();

    // Unchanged: metadata + samples match, no full hash.
    let full_hashes_before = full_hash_call_count();
    let status = SourceFingerprint::check_sqlite_path(file.path(), Some(&fingerprint)).unwrap();
    assert!(matches!(status, FingerprintStatus::Unchanged));

    // Changed: a same-size rewrite with a rolled-back mtime is still caught
    // by the samples, and still without a whole-file hash.
    let original_modified = std::fs::metadata(file.path()).unwrap().modified().unwrap();
    let mut rewritten = original;
    rewritten[0] = b'z';
    std::fs::write(file.path(), rewritten).unwrap();
    File::options()
        .write(true)
        .open(file.path())
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(original_modified))
        .unwrap();

    let status = SourceFingerprint::check_sqlite_path(file.path(), Some(&fingerprint)).unwrap();
    assert!(matches!(status, FingerprintStatus::Changed(_)));
    assert_eq!(
        full_hash_call_count(),
        full_hashes_before,
        "SQLite change detection must never compute a whole-file SHA-256"
    );
}

#[test]
fn test_source_fingerprint_changes_for_same_size_rewrite() {
    let file = write_temp_file(b"aaaa\nbbbb\ncccc\n");
    let before = SourceFingerprint::from_path(file.path()).unwrap();

    std::fs::write(file.path(), b"aaaa\nzzzz\ncccc\n").unwrap();

    let after = SourceFingerprint::from_path(file.path()).unwrap();
    assert_ne!(before, after);
}

#[test]
fn test_source_fingerprint_changes_for_large_same_size_unsampled_rewrite() {
    let mut original = vec![b'a'; 128 * 1024];
    original.extend_from_slice(b"\n");
    let file = write_temp_file(&original);
    let before = SourceFingerprint::from_path(file.path()).unwrap();

    let mut rewritten = original.clone();
    rewritten[73 * 1024] = b'z';
    std::fs::write(file.path(), &rewritten).unwrap();

    let after = SourceFingerprint::from_path(file.path()).unwrap();
    assert_ne!(before, after);
}

#[test]
fn test_sqlite_source_fingerprint_tracks_sidecar_changes() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("history.db");
    std::fs::write(&db_path, b"main-db").unwrap();

    let base = SourceFingerprint::from_sqlite_path(&db_path).unwrap();

    let wal_path = append_path_suffix(&db_path, "-wal");
    std::fs::write(&wal_path, b"wal-1").unwrap();
    let with_wal = SourceFingerprint::from_sqlite_path(&db_path).unwrap();
    assert_ne!(base, with_wal);

    std::fs::write(&wal_path, b"wal-2").unwrap();
    let updated_wal = SourceFingerprint::from_sqlite_path(&db_path).unwrap();
    assert_ne!(with_wal, updated_wal);

    let before_shm = SourceFingerprint::from_sqlite_path(&db_path).unwrap();
    let shm_path = append_path_suffix(&db_path, "-shm");
    std::fs::write(&shm_path, b"shm-1").unwrap();
    let with_shm = SourceFingerprint::from_sqlite_path(&db_path).unwrap();
    assert_eq!(before_shm, with_shm);
}

#[test]
fn test_codex_incremental_cache_requires_newline_boundary() {
    let file = write_temp_file(b"line-1\nline-2");

    assert!(build_codex_incremental_cache(
        file.path(),
        file.as_file().metadata().unwrap().len(),
        CodexParseState::default(),
    )
    .is_none());
}

#[test]
fn test_codex_prefix_matches_rejects_middle_rewrite_with_same_tail() {
    let file = write_temp_file(b"aaaa\nbbbb\ncccc\n");
    let fingerprint = SourceFingerprint::from_path(file.path()).unwrap();
    let incremental_cache =
        build_codex_incremental_cache(file.path(), fingerprint.size, CodexParseState::default())
            .unwrap();

    std::fs::write(file.path(), b"aaaa\nzzzz\ncccc\nmore\n").unwrap();

    assert!(!codex_prefix_matches(file.path(), &incremental_cache));
}

#[test]
fn test_codex_prefix_matches_rejects_large_unsampled_rewrite() {
    let mut original = vec![b'a'; 128 * 1024];
    original.extend_from_slice(b"\n");
    let file = write_temp_file(&original);
    let fingerprint = SourceFingerprint::from_path(file.path()).unwrap();
    let incremental_cache =
        build_codex_incremental_cache(file.path(), fingerprint.size, CodexParseState::default())
            .unwrap();

    let mut rewritten = original.clone();
    rewritten[73 * 1024] = b'z';
    rewritten.extend_from_slice(b"appended\n");
    std::fs::write(file.path(), rewritten).unwrap();

    assert!(!codex_prefix_matches(file.path(), &incremental_cache));
}
