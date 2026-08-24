use super::*;

#[test]
fn from_roo_path_invalidates_on_history_only_change() {
    // parse_roo_kilo_file reads model/agent from the sibling
    // api_conversation_history.json, so a history-only rewrite (ui_messages
    // byte-identical) must change the fingerprint or the cache serves stale
    // model/agent/pricing.
    let dir = TempDir::new().unwrap();
    let ui = dir.path().join("ui_messages.json");
    std::fs::write(&ui, b"[]").unwrap();
    let history = dir.path().join("api_conversation_history.json");
    std::fs::write(&history, b"<model>claude-sonnet-4</model>").unwrap();

    let roo_before = SourceFingerprint::from_roo_path(&ui).unwrap();
    let plain_before = SourceFingerprint::from_path(&ui).unwrap();

    // Rewrite the history only; leave ui_messages.json byte-identical.
    std::fs::write(&history, b"<model>claude-opus-4</model>").unwrap();

    let roo_after = SourceFingerprint::from_roo_path(&ui).unwrap();
    let plain_after = SourceFingerprint::from_path(&ui).unwrap();

    assert_ne!(
        roo_before, roo_after,
        "a history-only change must alter the roo fingerprint"
    );
    assert_eq!(
        plain_before, plain_after,
        "from_path ignores the history sibling (control)"
    );
}

#[test]
fn cline_cli_fingerprint_tracks_manifest_changes() {
    let dir = TempDir::new().unwrap();
    let messages = dir.path().join("session.messages.json");
    let manifest = dir.path().join("session.json");
    std::fs::write(&messages, br#"{"messages":[]}"#).unwrap();

    let initial = match SourceFingerprint::check_cline_path_samples_only(&messages, None) {
        Some(FingerprintStatus::Changed(fingerprint)) => fingerprint,
        other => panic!("expected an initial fingerprint, got {other:?}"),
    };
    assert!(initial.related_files.iter().any(|related| {
        related.suffix == "manifest.json"
            && related.path.to_path_buf() == manifest
            && !related.exists
    }));
    assert!(matches!(
        SourceFingerprint::check_cline_path_samples_only(&messages, Some(&initial)),
        Some(FingerprintStatus::Unchanged)
    ));

    std::fs::write(&manifest, br#"{"title":"first"}"#).unwrap();
    assert!(matches!(
        SourceFingerprint::check_cline_path_samples_only(&messages, Some(&initial)),
        Some(FingerprintStatus::Changed(_))
    ));

    let with_manifest =
        match SourceFingerprint::check_cline_path_samples_only(&messages, Some(&initial)) {
            Some(FingerprintStatus::Changed(fingerprint)) => fingerprint,
            other => panic!("expected a refreshed fingerprint, got {other:?}"),
        };
    std::fs::write(&manifest, br#"{"title":"second"}"#).unwrap();
    assert!(matches!(
        SourceFingerprint::check_cline_path_samples_only(&messages, Some(&with_manifest)),
        Some(FingerprintStatus::Changed(_))
    ));
}

#[test]
fn test_devin_desktop_fingerprint_tracks_cli_lookup_database_and_wal() {
    let dir = TempDir::new().unwrap();
    let desktop_path = dir.path().join("desktop.ndjson");
    let db_path = dir.path().join("sessions.db");
    std::fs::write(&desktop_path, b"desktop usage\n").unwrap();
    std::fs::write(&db_path, b"lookup-one").unwrap();

    let fingerprint = match SourceFingerprint::check_devin_desktop_path_samples_only(
        &desktop_path,
        std::slice::from_ref(&db_path),
        None,
    )
    .unwrap()
    {
        FingerprintStatus::Changed(fingerprint) => fingerprint,
        FingerprintStatus::Unchanged => panic!("an uncached source must build a fingerprint"),
    };
    assert!(matches!(
        SourceFingerprint::check_devin_desktop_path_samples_only(
            &desktop_path,
            std::slice::from_ref(&db_path),
            Some(&fingerprint),
        ),
        Some(FingerprintStatus::Unchanged)
    ));

    std::fs::write(&db_path, b"lookup-two").unwrap();
    let changed = SourceFingerprint::check_devin_desktop_path_samples_only(
        &desktop_path,
        std::slice::from_ref(&db_path),
        Some(&fingerprint),
    )
    .unwrap();
    let fingerprint = match changed {
        FingerprintStatus::Changed(fingerprint) => fingerprint,
        FingerprintStatus::Unchanged => panic!("a lookup database rewrite must invalidate"),
    };

    std::fs::write(append_path_suffix(&db_path, "-wal"), b"wal").unwrap();
    assert!(matches!(
        SourceFingerprint::check_devin_desktop_path_samples_only(
            &desktop_path,
            std::slice::from_ref(&db_path),
            Some(&fingerprint),
        ),
        Some(FingerprintStatus::Changed(_))
    ));
}

#[test]
fn test_jcode_fingerprint_tracks_journal_sidecar_changes() {
    let dir = TempDir::new().unwrap();
    let session_path = dir.path().join("session_fixture.json");
    std::fs::write(&session_path, br#"{"messages":[]}"#).unwrap();

    let base = SourceFingerprint::from_jcode_path(&session_path).unwrap();

    let journal_path = dir.path().join("session_fixture.journal.jsonl");
    std::fs::write(
        &journal_path,
        br#"{"append_messages":[]}
"#,
    )
    .unwrap();
    let with_journal = SourceFingerprint::from_jcode_path(&session_path).unwrap();
    assert_ne!(base, with_journal);

    std::fs::write(
        &journal_path,
        br#"{"append_messages":[{"id":"assistant_1"}]}
"#,
    )
    .unwrap();
    let updated_journal = SourceFingerprint::from_jcode_path(&session_path).unwrap();
    assert_ne!(with_journal, updated_journal);
}

#[test]
fn test_grok_fingerprint_tracks_signals_sidecar_changes() {
    let dir = TempDir::new().unwrap();
    let updates_path = dir.path().join("updates.jsonl");
    std::fs::write(&updates_path, b"update\n").unwrap();

    let base = SourceFingerprint::from_grok_path(&updates_path).unwrap();

    let signals_path = dir.path().join("signals.json");
    std::fs::write(&signals_path, br#"{"input":1}"#).unwrap();
    let with_signals = SourceFingerprint::from_grok_path(&updates_path).unwrap();
    assert_ne!(base, with_signals);

    std::fs::write(&signals_path, br#"{"input":2}"#).unwrap();
    let updated_signals = SourceFingerprint::from_grok_path(&updates_path).unwrap();
    assert_ne!(with_signals, updated_signals);
}

#[test]
fn test_grok_fingerprint_tracks_summary_and_events_sidecar_changes() {
    let dir = TempDir::new().unwrap();
    let updates_path = dir.path().join("updates.jsonl");
    std::fs::write(&updates_path, b"update\n").unwrap();

    let base = SourceFingerprint::from_grok_path(&updates_path).unwrap();

    let summary_path = dir.path().join("summary.json");
    std::fs::write(&summary_path, br#"{"model":"grok-3"}"#).unwrap();
    let with_summary = SourceFingerprint::from_grok_path(&updates_path).unwrap();
    assert_ne!(base, with_summary);

    std::fs::write(&summary_path, br#"{"model":"grok-4"}"#).unwrap();
    let updated_summary = SourceFingerprint::from_grok_path(&updates_path).unwrap();
    assert_ne!(with_summary, updated_summary);

    let events_path = dir.path().join("events.jsonl");
    std::fs::write(&events_path, b"event-1\n").unwrap();
    let with_events = SourceFingerprint::from_grok_path(&updates_path).unwrap();
    assert_ne!(updated_summary, with_events);

    std::fs::write(&events_path, b"event-2\n").unwrap();
    let updated_events = SourceFingerprint::from_grok_path(&updates_path).unwrap();
    assert_ne!(with_events, updated_events);
}

#[test]
fn test_reasonix_stats_fingerprint_tracks_appends() {
    let dir = TempDir::new().unwrap();
    let session_path = dir.path().join("2026-08-04.jsonl");
    std::fs::write(&session_path, b"{\"total\":1}\n").unwrap();

    let initial = match SourceFingerprint::check_reasonix_path_samples_only(&session_path, None) {
        Some(FingerprintStatus::Changed(fingerprint)) => fingerprint,
        _ => panic!("uncached Reasonix session must produce a fingerprint"),
    };
    assert!(matches!(
        SourceFingerprint::check_reasonix_path_samples_only(&session_path, Some(&initial)),
        Some(FingerprintStatus::Unchanged)
    ));

    std::fs::write(&session_path, b"{\"total\":1}\n{\"total\":2}\n").unwrap();
    match SourceFingerprint::check_reasonix_path_samples_only(&session_path, Some(&initial)) {
        Some(FingerprintStatus::Changed(fingerprint)) => fingerprint,
        _ => panic!("Reasonix stats append must invalidate"),
    };
}

#[test]
fn test_grok_unified_fingerprint_tracks_session_metadata_tree_changes() {
    let dir = TempDir::new().unwrap();
    let logs_dir = dir.path().join(".grok/logs");
    let session_dir = dir.path().join(".grok/sessions/%2Ftmp%2Fproject/session-1");
    std::fs::create_dir_all(&logs_dir).unwrap();
    std::fs::create_dir_all(&session_dir).unwrap();
    let unified_path = logs_dir.join("unified.jsonl");
    std::fs::write(
        &unified_path,
        br#"{"sid":"session-1","msg":"shell.turn.inference_done","ctx":{"prompt_tokens":1,"completion_tokens":1}}"#,
    )
    .unwrap();
    let summary_path = session_dir.join("summary.json");
    std::fs::write(&summary_path, br#"{"current_model_id":"grok-4.5"}"#).unwrap();

    let base = SourceFingerprint::from_grok_path(&unified_path).unwrap();

    std::fs::write(&summary_path, br#"{"current_model_id":"grok-4.6"}"#).unwrap();
    let changed_summary = SourceFingerprint::from_grok_path(&unified_path).unwrap();
    assert_ne!(base, changed_summary);
    assert!(matches!(
        SourceFingerprint::check_grok_path_samples_only(&unified_path, Some(&base)),
        Some(FingerprintStatus::Changed(_))
    ));

    let second_session_dir = dir.path().join(".grok/sessions/%2Ftmp%2Fproject/session-2");
    std::fs::create_dir_all(&second_session_dir).unwrap();
    std::fs::write(
        second_session_dir.join("summary.json"),
        br#"{"current_model_id":"grok-4.7"}"#,
    )
    .unwrap();
    let changed_tree = SourceFingerprint::from_grok_path(&unified_path).unwrap();
    assert_ne!(changed_summary, changed_tree);
}

#[test]
fn test_kiro_ide_fingerprint_tracks_messages_sidecar_changes() {
    let dir = TempDir::new().unwrap();
    let sess_dir = dir.path().join("workspace-a/sess_02f1c107");
    std::fs::create_dir_all(&sess_dir).unwrap();
    let session_path = sess_dir.join("session.json");
    std::fs::write(&session_path, br#"{"schemaVersion":"1.0.0"}"#).unwrap();

    let base = SourceFingerprint::from_kiro_path(&session_path).unwrap();

    // messages.jsonl appearing (session.json untouched) must invalidate.
    let messages_path = sess_dir.join("messages.jsonl");
    std::fs::write(
        &messages_path,
        br#"{"role":"user","content":"hello"}
"#,
    )
    .unwrap();
    let with_messages = SourceFingerprint::from_kiro_path(&session_path).unwrap();
    assert_ne!(base, with_messages);

    // An append landing after the last session.json write must invalidate.
    std::fs::write(
        &messages_path,
        br#"{"role":"user","content":"hello"}
{"role":"assistant","content":"world"}
"#,
    )
    .unwrap();
    let updated_messages = SourceFingerprint::from_kiro_path(&session_path).unwrap();
    assert_ne!(with_messages, updated_messages);

    // A CLI source records its absent same-stem JSONL sidecar so a later
    // creation invalidates the cache without reparsing the primary file.
    let cli_path = dir.path().join("cli-session.json");
    std::fs::write(&cli_path, b"{}").unwrap();
    let cli_fingerprint = SourceFingerprint::from_kiro_path(&cli_path).unwrap();
    assert!(cli_fingerprint.related_files.iter().any(|related| {
        related.suffix == "messages.jsonl"
            && related.path.to_path_buf() == dir.path().join("cli-session.jsonl")
            && !related.exists
    }));
}

#[test]
fn test_kiro_cli_fingerprint_tracks_same_stem_jsonl_changes() {
    let dir = TempDir::new().unwrap();
    let session_path = dir.path().join("cli-session.json");
    std::fs::write(&session_path, br#"{"sessionId":"session-1"}"#).unwrap();

    let base = SourceFingerprint::from_kiro_path(&session_path).unwrap();

    let messages_path = dir.path().join("cli-session.jsonl");
    std::fs::write(&messages_path, b"message-1\n").unwrap();
    let with_messages = SourceFingerprint::from_kiro_path(&session_path).unwrap();
    assert_ne!(base, with_messages);

    std::fs::write(&messages_path, b"message-2\n").unwrap();
    let updated_messages = SourceFingerprint::from_kiro_path(&session_path).unwrap();
    assert_ne!(with_messages, updated_messages);
}

#[test]
fn test_droid_fingerprint_tracks_fallback_jsonl_changes() {
    let dir = TempDir::new().unwrap();
    let settings_path = dir.path().join("session.settings.json");
    std::fs::write(&settings_path, br#"{"tokenUsage":{"inputTokens":1}}"#).unwrap();

    let base = SourceFingerprint::from_droid_path(&settings_path).unwrap();

    let jsonl_path = dir.path().join("session.jsonl");
    std::fs::write(&jsonl_path, b"Model: Claude Sonnet 4\n").unwrap();
    let with_jsonl = SourceFingerprint::from_droid_path(&settings_path).unwrap();
    assert_ne!(base, with_jsonl);

    std::fs::write(&jsonl_path, b"Model: Claude Opus 4\n").unwrap();
    let updated_jsonl = SourceFingerprint::from_droid_path(&settings_path).unwrap();
    assert_ne!(with_jsonl, updated_jsonl);
}

#[test]
fn test_kimi_fingerprint_tracks_legacy_config_but_keeps_kimi_code_self_contained() {
    let dir = TempDir::new().unwrap();
    let legacy_path = dir.path().join(".kimi/sessions/group/session/wire.jsonl");
    std::fs::create_dir_all(legacy_path.parent().unwrap()).unwrap();
    std::fs::write(&legacy_path, b"usage\n").unwrap();

    let legacy_base = SourceFingerprint::from_kimi_path(&legacy_path).unwrap();
    let legacy_config = dir.path().join(".kimi/config.json");
    std::fs::write(&legacy_config, br#"{"model":"kimi-k2"}"#).unwrap();
    let legacy_with_config = SourceFingerprint::from_kimi_path(&legacy_path).unwrap();
    assert_ne!(legacy_base, legacy_with_config);

    std::fs::write(&legacy_config, br#"{"model":"kimi-k3"}"#).unwrap();
    let legacy_updated_config = SourceFingerprint::from_kimi_path(&legacy_path).unwrap();
    assert_ne!(legacy_with_config, legacy_updated_config);

    let code_path = dir
        .path()
        .join(".kimi-code/sessions/workspace/session/agents/main/wire.jsonl");
    std::fs::create_dir_all(code_path.parent().unwrap()).unwrap();
    std::fs::write(&code_path, b"usage.record\n").unwrap();
    let code_base = SourceFingerprint::from_kimi_path(&code_path).unwrap();
    assert_eq!(code_base, SourceFingerprint::from_path(&code_path).unwrap());

    let would_be_config = crate::sessions::kimi::kimi_config_path(&code_path).unwrap();
    std::fs::create_dir_all(would_be_config.parent().unwrap()).unwrap();
    std::fs::write(&would_be_config, br#"{"model":"unrelated"}"#).unwrap();
    let code_with_config = SourceFingerprint::from_kimi_path(&code_path).unwrap();
    assert_eq!(code_base, code_with_config);
}
