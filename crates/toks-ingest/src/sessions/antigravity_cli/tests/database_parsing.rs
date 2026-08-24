use super::*;

fn build_trajectory_meta() -> Vec<u8> {
    let workspace = enc_len(1, b"file:///C:/Users/Frank/obsidian-vault");
    let created = {
        let mut created = Vec::new();
        created.extend(enc_varint(1, 1_781_502_653)); // seconds
        created.extend(enc_varint(2, 0)); // nanos
        created
    };
    let mut blob = Vec::new();
    blob.extend(enc_len(1, &workspace));
    blob.extend(enc_len(2, &created));
    blob
}

#[test]
fn overlarge_varint_token_counts_are_clamped_not_wrapped() {
    // A corrupt/malicious blob encoding a varint > i64::MAX must clamp to a
    // non-negative i64 (saturating), never wrap `as i64` to a negative count.
    let mut usage = Vec::new();
    usage.extend(enc_varint(1, u64::MAX)); // huge fixed system prompt
    usage.extend(enc_varint(2, 10)); // + small input -> saturating_add
    usage.extend(enc_varint(9, u64::MAX)); // huge output
    usage.extend(enc_len(11, b"resp-overflow"));
    let mut chat_model = Vec::new();
    chat_model.extend(enc_len(4, &usage));
    chat_model.extend(enc_len(19, b"gemini-3-flash-a"));
    let blob = enc_len(1, &chat_model);

    let mut seen = HashSet::new();
    let msg = parse_isolated_row(&blob, "s", 1_000, &mut seen).expect("parses");
    assert_eq!(msg.tokens.output, i64::MAX);
    assert_eq!(msg.tokens.input, i64::MAX); // saturating_add, not negative
    assert!(msg.tokens.input >= 0 && msg.tokens.output >= 0);
}

#[test]
fn parses_tokens_model_and_workspace_from_db() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session-test.db");

    {
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE gen_metadata (idx integer, data blob, size integer);
             CREATE TABLE trajectory_metadata_blob (id text, data blob);",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO gen_metadata (idx, data, size) VALUES (0, ?1, 0)",
            params![build_gen_metadata()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO trajectory_metadata_blob (id, data) VALUES ('main', ?1)",
            params![build_trajectory_meta()],
        )
        .unwrap();
    }

    let messages = parse_antigravity_cli_file(&path);
    assert_eq!(messages.len(), 1);
    let message = &messages[0];
    assert_eq!(message.client, "antigravity-cli");
    // `gemini-3-flash-a` (raw #19 responseModel) is alias-resolved to the
    // priced canonical model so cost lookups don't fall through to 0.
    // Per upstream (models.ts@603e3ea), `gemini-3-flash-a` is the legacy
    // responseModel for M132, the retired predecessor of M133 — i.e. the
    // High tier, not the unrelated gemini-3-flash-preview family.
    assert_eq!(message.model_id, "gemini-3.5-flash-high");
    assert_eq!(message.provider_id, "google");
    assert_eq!(message.session_id, "session-test");
    assert_eq!(message.tokens.input, 1632); // 1132 + 500
    assert_eq!(message.tokens.cache_read, 16000);
    assert_eq!(message.tokens.output, 300);
    assert_eq!(message.tokens.reasoning, 40);
    assert_eq!(message.dedup_key.as_deref(), Some("resp-1"));
    assert_eq!(message.timestamp, 1_781_502_653_000);
    assert_eq!(
        message.workspace_key.as_deref(),
        Some("C:/Users/Frank/obsidian-vault")
    );
    assert_eq!(message.workspace_label.as_deref(), Some("obsidian-vault"));
}
