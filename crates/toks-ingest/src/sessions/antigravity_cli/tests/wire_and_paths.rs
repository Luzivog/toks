use super::*;

#[test]
fn malformed_blob_returns_none_without_panic() {
    let mut seen = HashSet::new();
    // Empty buffer: no chatModel sub-message.
    assert!(parse_isolated_row(&[], "s", 0, &mut seen).is_none());
    // Garbage bytes that do not form a valid wire-format message.
    assert!(parse_isolated_row(&[0xff, 0xff, 0xff, 0xff], "s", 0, &mut seen).is_none());
    // A length-delimited #1 whose declared length overruns the buffer:
    // exercises the ProtoReader bounds check (must stop, not index OOB).
    let truncated = [(1u8 << 3) | 2, 0x7f, 0x01, 0x02];
    assert!(parse_isolated_row(&truncated, "s", 0, &mut seen).is_none());
    // Valid outer #1 wrapping a #4 usage whose declared length overruns:
    // the inner reader must bail without panicking.
    let inner = [(4u8 << 3) | 2, 0x40, 0x00];
    let mut outer = vec![(1u8 << 3) | 2, inner.len() as u8];
    outer.extend_from_slice(&inner);
    assert!(parse_isolated_row(&outer, "s", 0, &mut seen).is_none());
}

#[test]
fn proto_timestamp_ms_overflow_returns_none_without_panic() {
    // A malformed Timestamp can carry a `seconds` varint whose `* 1000`
    // overflows i64. Debug builds (overflow-checks = on) would panic on the
    // unchecked multiply; the decode must degrade to None instead, matching
    // the module's malformed-data contract.
    let mut overflow = Vec::new();
    overflow.extend(enc_varint(1, i64::MAX as u64)); // seconds -> *1000 overflows
    overflow.extend(enc_varint(2, 0)); // nanos
    assert_eq!(proto_timestamp_ms(&overflow), None);

    // The boundary case: largest `seconds` whose *1000 still fits i64 must
    // decode, proving the guard rejects only genuine overflow.
    let ok_seconds = i64::MAX / 1000;
    let mut ok = Vec::new();
    ok.extend(enc_varint(1, ok_seconds as u64));
    ok.extend(enc_varint(2, 0));
    assert_eq!(proto_timestamp_ms(&ok), Some(ok_seconds * 1000));

    // A normal, in-range stamp still decodes (seconds + nanos -> ms).
    let mut normal = Vec::new();
    normal.extend(enc_varint(1, 1_781_000_000));
    normal.extend(enc_varint(2, 250_000_000)); // +250ms
    assert_eq!(
        proto_timestamp_ms(&normal),
        Some(1_781_000_000 * 1000 + 250)
    );
}

#[test]
fn proto_timestamp_ms_rejects_out_of_range_nanos() {
    // The protobuf Timestamp spec requires `nanos` in 0..=999_999_999.
    // An out-of-range `nanos` marks the stamp malformed (None) rather than
    // producing a skewed time. 1_000_000_000 (== one extra second) is the
    // first invalid value above the inclusive upper bound.
    let mut bad_nanos = Vec::new();
    bad_nanos.extend(enc_varint(1, 1_781_000_000)); // valid seconds
    bad_nanos.extend(enc_varint(2, 1_000_000_000)); // nanos out of range
    assert_eq!(proto_timestamp_ms(&bad_nanos), None);

    // A nanos varint large enough to be negative once cast to i64 is also
    // rejected (never wraps to a bogus negative offset).
    let mut huge_nanos = Vec::new();
    huge_nanos.extend(enc_varint(1, 1_781_000_000));
    huge_nanos.extend(enc_varint(2, u64::MAX));
    assert_eq!(proto_timestamp_ms(&huge_nanos), None);

    // The inclusive upper bound is accepted (999_999_999 ns -> +999 ms).
    let mut max_nanos = Vec::new();
    max_nanos.extend(enc_varint(1, 1_781_000_000));
    max_nanos.extend(enc_varint(2, 999_999_999));
    assert_eq!(
        proto_timestamp_ms(&max_nanos),
        Some(1_781_000_000 * 1000 + 999)
    );

    // End-to-end: a gen_metadata row whose #9.#4 carries out-of-range nanos
    // must fall back to the session-created timestamp (the caller's invalid
    // -> None -> session fallback path), not adopt a skewed per-turn stamp.
    let session_fallback = 222_000_i64;
    let mut usage = Vec::new();
    usage.extend(enc_varint(2, 500)); // input
    usage.extend(enc_varint(9, 300)); // output
    usage.extend(enc_len(11, b"bad-nanos")); // responseId

    let mut gen_time = Vec::new();
    gen_time.extend(enc_varint(1, 1_781_000_000)); // seconds
    gen_time.extend(enc_varint(2, 1_000_000_000)); // nanos out of range
    let gen9 = enc_len(4, &gen_time);

    let mut chat_model = Vec::new();
    chat_model.extend(enc_len(4, &usage));
    chat_model.extend(enc_len(9, &gen9));
    chat_model.extend(enc_len(19, b"gemini-3-flash-a"));
    let blob = enc_len(1, &chat_model);

    let mut seen = HashSet::new();
    let message = parse_isolated_row(&blob, "s", session_fallback, &mut seen).unwrap();
    assert_eq!(
        message.timestamp, session_fallback,
        "out-of-range per-generation nanos must fall back to the session timestamp"
    );
}

#[test]
fn file_uri_to_path_handles_windows_posix_and_unc() {
    // Empty authority + Windows drive: drop the slash before the drive.
    assert_eq!(
        file_uri_to_path("file:///C:/Users/Frank/obsidian-vault").as_deref(),
        Some("C:/Users/Frank/obsidian-vault")
    );
    // Empty authority + POSIX absolute: keep as-is.
    assert_eq!(
        file_uri_to_path("file:///home/frank/project").as_deref(),
        Some("/home/frank/project")
    );
    // Non-empty authority is a UNC path; the host must survive as `//host`.
    assert_eq!(
        file_uri_to_path("file://server/share/code").as_deref(),
        Some("//server/share/code")
    );
    // Percent-encoded UTF-8 (CJK) decodes to valid characters.
    assert_eq!(
        file_uri_to_path("file:///D:/%E6%88%91%E7%9A%84").as_deref(),
        Some("D:/我的")
    );
    // Anything without the scheme prefix is rejected.
    assert_eq!(file_uri_to_path("not-a-file-uri"), None);
}
