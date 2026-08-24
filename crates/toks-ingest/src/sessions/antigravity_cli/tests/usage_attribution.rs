use super::*;

#[test]
fn per_generation_timestamp_overrides_session_fallback() {
    // chatModel.#9.#4 = {#1: seconds, #2: nanos} is the per-turn wall-clock
    // stamp. When present it dates the row; when absent the row falls back
    // to the session-created timestamp passed in. (Verified against real
    // databases: every gen_metadata row carries a distinct, monotonic
    // #9.#4 stamp >= the session-created time.)
    let session_fallback = 111_000_i64;

    let mut usage = Vec::new();
    usage.extend(enc_varint(2, 500)); // input
    usage.extend(enc_varint(9, 300)); // output
    usage.extend(enc_len(11, b"with-time")); // responseId

    // #9 wraps a sub-message whose #4 is the {seconds, nanos} Timestamp.
    let mut gen_time = Vec::new();
    gen_time.extend(enc_varint(1, 1_781_000_000)); // seconds
    gen_time.extend(enc_varint(2, 250_000_000)); // nanos -> +250ms
    let gen9 = enc_len(4, &gen_time);

    let mut chat_model = Vec::new();
    chat_model.extend(enc_len(4, &usage));
    chat_model.extend(enc_len(9, &gen9));
    chat_model.extend(enc_len(19, b"gemini-3-flash-a"));
    let blob = enc_len(1, &chat_model);

    let mut seen = HashSet::new();
    let message = parse_isolated_row(&blob, "s", session_fallback, &mut seen).unwrap();
    assert_eq!(
        message.timestamp,
        1_781_000_000 * 1000 + 250,
        "per-generation #9.#4 timestamp must override the session fallback"
    );

    // The same row shape without #9 falls back to the session timestamp
    // (build_gen_metadata carries no #9.#4).
    let mut seen2 = HashSet::new();
    let fallback_msg =
        parse_isolated_row(&build_gen_metadata(), "s", session_fallback, &mut seen2).unwrap();
    assert_eq!(
        fallback_msg.timestamp, session_fallback,
        "a row without #9.#4 must use the session-created fallback"
    );
}

#[test]
fn dedupes_repeated_response_ids_and_skips_zero_usage() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dupes.db");

    // Two rows share responseId "dup"; a third row has all-zero usage.
    let mut zero_usage = Vec::new();
    zero_usage.extend(enc_len(11, b"zero"));
    let mut zero_chat = Vec::new();
    zero_chat.extend(enc_len(4, &zero_usage));
    zero_chat.extend(enc_len(19, b"gemini-3-flash-a"));
    let zero_blob = enc_len(1, &zero_chat);

    {
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch("CREATE TABLE gen_metadata (idx integer, data blob, size integer);")
            .unwrap();
        for (idx, blob) in [
            (0, build_gen_metadata()),
            (1, build_gen_metadata()),
            (2, zero_blob),
        ] {
            conn.execute(
                "INSERT INTO gen_metadata (idx, data, size) VALUES (?1, ?2, 0)",
                params![idx, blob],
            )
            .unwrap();
        }
    }

    let messages = parse_antigravity_cli_file(&path);
    // Only the first "resp-1" row survives; the duplicate and the
    // zero-usage row are dropped. Missing trajectory_metadata_blob table is
    // tolerated (timestamp falls back to file mtime).
    assert_eq!(messages.len(), 1);
    assert!(messages[0].timestamp > 0);
}

#[test]
fn emitted_model_string_resolves_to_priced_alias() {
    // The parser emits the raw `#19` responseModel (`gemini-3-flash-a`) and
    // relies on the alias table to map it onto a priced model. Without the
    // alias the cost would resolve to 0, so lock the resolution here at the
    // unit level (an end-to-end calculate_cost path needs the live pricing
    // dataset, which is unavailable in unit tests).
    assert_eq!(
        pricing::aliases::resolve_alias("gemini-3-flash-a"),
        Some("gemini-3.5-flash-high")
    );
}

#[test]
fn output_and_thinking_map_to_fields_9_and_10() {
    // Lock the field-mapping contract asserted by the module doc-comment:
    // `#9 + #10 == #3` (output + thinking == stored total output). Build a
    // synthetic blob where #9=output, #10=thinking, #3=output+thinking and
    // verify the parsed message keeps #9 as output and #10 as reasoning.
    let output = 300u64;
    let thinking = 40u64;
    let total_output = output + thinking; // #3

    let mut usage = Vec::new();
    usage.extend(enc_varint(1, 1132)); // fixed system prompt
    usage.extend(enc_varint(2, 500)); // new input
    usage.extend(enc_varint(3, total_output)); // stored total output (#3)
    usage.extend(enc_varint(9, output)); // output (#9)
    usage.extend(enc_varint(10, thinking)); // thinking (#10)
    usage.extend(enc_len(11, b"invariant-1"));

    let mut chat_model = Vec::new();
    chat_model.extend(enc_len(4, &usage));
    chat_model.extend(enc_len(19, b"gemini-3-flash-a"));
    let blob = enc_len(1, &chat_model);

    let mut seen = HashSet::new();
    let message = parse_isolated_row(&blob, "session", 0, &mut seen).unwrap();
    assert_eq!(message.tokens.output, output as i64);
    assert_eq!(message.tokens.reasoning, thinking as i64);
    // The contract: the two component fields sum to the stored total.
    assert_eq!(
        (message.tokens.output + message.tokens.reasoning) as u64,
        total_output
    );
}
