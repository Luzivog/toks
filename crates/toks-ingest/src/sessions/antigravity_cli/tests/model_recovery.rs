use super::*;

/// Build a conversation database from `gen_metadata` blobs in row order.
fn write_conversation(path: &Path, blobs: &[Vec<u8>]) {
    let conn = Connection::open(path).unwrap();
    conn.execute_batch("CREATE TABLE gen_metadata (idx integer, data blob, size integer);")
        .unwrap();
    for (idx, blob) in blobs.iter().enumerate() {
        conn.execute(
            "INSERT INTO gen_metadata (idx, data, size) VALUES (?1, ?2, 0)",
            params![idx as i64, blob],
        )
        .unwrap();
    }
}

#[test]
fn resolves_current_antigravity_cli_response_model() {
    let blob = build_gen_metadata_with_model("gemini-3-flash-agent");
    let mut seen = HashSet::new();

    let message = parse_isolated_row(&blob, "session", 1_000, &mut seen).unwrap();

    assert_eq!(message.model_id, "gemini-3.5-flash-high");
    assert_eq!(message.provider_id, "google");
}

// The generic routing label is preserved verbatim. It is not a concrete
// billable model id, so submit can exclude it instead of inventing a cost.
#[test]
fn gemini_default_response_model_is_preserved() {
    let blob = build_gen_metadata_with_model("gemini-default");
    let mut seen = HashSet::new();

    let message = parse_isolated_row(&blob, "session", 1_000, &mut seen).unwrap();

    assert_eq!(message.model_id, "gemini-default");
    assert_eq!(message.provider_id, "google");
}

// Antigravity CLI omits `#19` on some continuation turns while still
// writing `#21`. Observed in three real conversations: every such row sat
// in a database whose other rows carried `#19` next to the identical `#21`
// label, so the machine id is recoverable and the row must not degrade to
// the unpriceable `antigravity/unknown`.
#[test]
fn missing_response_model_is_recovered_from_the_display_label() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("continuation.db");
    write_conversation(
        &path,
        &[
            build_row(
                Some("gemini-3.6-flash"),
                Some("Gemini 3.6 Flash (High)"),
                "resp-0",
            ),
            build_row(None, Some("Gemini 3.6 Flash (High)"), "resp-1"),
        ],
    );

    let messages = parse_antigravity_cli_file(&path);
    assert_eq!(messages.len(), 2);
    for message in &messages {
        assert_eq!(message.model_id, "gemini-3.6-flash");
        assert_eq!(message.provider_id, "google");
    }
}

#[test]
fn recovery_reads_the_whole_conversation_not_just_earlier_rows() {
    // The index is built from every row before any row is parsed, so a
    // conversation whose first turn is the one missing `#19` recovers just
    // as well as one where the gap comes later.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gap-first.db");
    write_conversation(
        &path,
        &[
            build_row(None, Some("Gemini 3.6 Flash (High)"), "resp-0"),
            build_row(
                Some("gemini-3.6-flash"),
                Some("Gemini 3.6 Flash (High)"),
                "resp-1",
            ),
        ],
    );

    let messages = parse_antigravity_cli_file(&path);
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].model_id, "gemini-3.6-flash");
}

#[test]
fn recovered_model_still_resolves_through_the_alias_table() {
    // The recovered value is the raw `#19` wire string, so it must take the
    // same alias path as a directly-read one — otherwise recovery would
    // hand pricing an id it cannot match.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("aliased.db");
    write_conversation(
        &path,
        &[
            build_row(
                Some("gemini-3-flash-a"),
                Some("Gemini 3.5 Flash (High)"),
                "resp-0",
            ),
            build_row(None, Some("Gemini 3.5 Flash (High)"), "resp-1"),
        ],
    );

    let messages = parse_antigravity_cli_file(&path);
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[1].model_id, "gemini-3.5-flash-high");
    assert_eq!(messages[1].provider_id, "google");
}

#[test]
fn a_display_label_no_row_identified_is_not_guessed_at() {
    // The conversation switched models: the row missing `#19` is labelled
    // Pro, and the only identified model is a Flash. Borrowing the Flash id
    // would bill the turn at the wrong tier, so the row stays `unknown` —
    // a label alone is not a model id.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("switched.db");
    write_conversation(
        &path,
        &[
            build_row(
                Some("gemini-3.6-flash"),
                Some("Gemini 3.6 Flash (High)"),
                "resp-0",
            ),
            build_row(None, Some("Gemini 3.1 Pro"), "resp-1"),
        ],
    );

    let messages = parse_antigravity_cli_file(&path);
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].model_id, "gemini-3.6-flash");
    assert_eq!(messages[1].model_id, "unknown");
    assert_eq!(messages[1].provider_id, "antigravity");
}

#[test]
fn a_display_label_used_by_two_models_is_not_used_for_recovery() {
    // Should a label ever be reused across machine ids (a rename landing
    // mid-conversation), it identifies nothing and must be discarded rather
    // than resolved to whichever row happened to be indexed last.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ambiguous.db");
    write_conversation(
        &path,
        &[
            build_row(Some("gemini-3-flash-a"), Some("Gemini Flash"), "resp-0"),
            build_row(Some("gemini-3.6-flash"), Some("Gemini Flash"), "resp-1"),
            build_row(None, Some("Gemini Flash"), "resp-2"),
        ],
    );

    let messages = parse_antigravity_cli_file(&path);
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[2].model_id, "unknown");
}

#[test]
fn two_spellings_of_one_priced_model_are_not_an_ambiguous_label() {
    // Antigravity swaps machine ids mid-conversation without changing the
    // display label: `gemini-pro-default` and `gemini-pro-agent` are both
    // "Gemini 3.1 Pro (High)" and both price as `gemini-3.1-pro`. Comparing
    // the raw ids called that ambiguous and discarded the label, so the
    // continuation row below resolved to `unknown` — which has no pricing
    // and aborted `tokscope submit` outright (#1058).
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("aliased.db");
    write_conversation(
        &path,
        &[
            build_row(
                Some("gemini-pro-default"),
                Some("Gemini 3.1 Pro (High)"),
                "resp-0",
            ),
            build_row(
                Some("gemini-pro-agent"),
                Some("Gemini 3.1 Pro (High)"),
                "resp-1",
            ),
            build_row(None, Some("Gemini 3.1 Pro (High)"), "resp-2"),
        ],
    );

    let messages = parse_antigravity_cli_file(&path);
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[2].model_id, "gemini-3.1-pro");
    assert_ne!(messages[2].model_id, "unknown");
}

#[test]
fn a_row_with_no_model_fields_falls_back_to_a_single_model_conversation() {
    // Nothing joins a row that carries neither `#19` nor `#21`. When the
    // whole conversation used one model there is only one answer it could
    // have; when it used several, there is no answer and the row stays
    // `unknown`.
    let dir = tempfile::tempdir().unwrap();

    let single = dir.path().join("single-model.db");
    write_conversation(
        &single,
        &[
            build_row(
                Some("gemini-3.6-flash"),
                Some("Gemini 3.6 Flash (High)"),
                "resp-0",
            ),
            build_row(None, None, "resp-1"),
        ],
    );
    let messages = parse_antigravity_cli_file(&single);
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[1].model_id, "gemini-3.6-flash");

    let mixed = dir.path().join("mixed-models.db");
    write_conversation(
        &mixed,
        &[
            build_row(Some("gemini-3.6-flash"), None, "resp-0"),
            build_row(Some("gemini-3.1-pro"), None, "resp-1"),
            build_row(None, None, "resp-2"),
        ],
    );
    let messages = parse_antigravity_cli_file(&mixed);
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[2].model_id, "unknown");
}

#[test]
fn a_label_no_row_identified_withholds_the_sole_model_fallback() {
    // Only one model is named here, but the Pro-labelled row proves a second
    // one ran. A row carrying no fields at all could be either, so counting
    // named ids alone would bill a model switch under the wrong model.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("unnamed-second-model.db");
    write_conversation(
        &path,
        &[
            build_row(
                Some("gemini-3.6-flash"),
                Some("Gemini 3.6 Flash (High)"),
                "resp-0",
            ),
            build_row(None, Some("Gemini 3.1 Pro"), "resp-1"),
            build_row(None, None, "resp-2"),
        ],
    );

    let messages = parse_antigravity_cli_file(&path);
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0].model_id, "gemini-3.6-flash");
    assert_eq!(messages[1].model_id, "unknown");
    assert_eq!(messages[2].model_id, "unknown");
}
