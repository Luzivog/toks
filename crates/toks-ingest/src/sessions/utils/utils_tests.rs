use super::*;

#[test]
fn lossy_lines_survives_undecodable_bytes_and_strips_a_bom() {
    let raw: &[u8] = b"\xef\xbb\xbffirst\r\nse\xffcond\nthird";
    let lines: Vec<String> = lossy_lines(raw).collect();
    assert_eq!(lines, vec!["first", "se\u{fffd}cond", "third"]);
}

#[test]
fn lossy_lines_keeps_empty_lines_and_ends_at_eof() {
    let raw: &[u8] = b"a\n\nb\n";
    let lines: Vec<String> = lossy_lines(raw).collect();
    assert_eq!(lines, vec!["a", "", "b"]);
}

#[test]
fn parse_timestamp_value_rejects_zero_and_negative_numbers() {
    assert!(parse_timestamp_value(&serde_json::json!(0)).is_none());
    assert!(parse_timestamp_value(&serde_json::json!(-1000)).is_none());
    assert!(parse_timestamp_value(&serde_json::json!(-1_700_000_000_000_i64)).is_none());
}

#[test]
fn parse_timestamp_value_accepts_positive_numbers() {
    assert_eq!(
        parse_timestamp_value(&serde_json::json!(1_700_000_000_000_i64)),
        Some(1_700_000_000_000)
    );
    assert_eq!(
        parse_timestamp_value(&serde_json::json!(1_700_000_000_i64)),
        Some(1_700_000_000_000)
    );
}

#[test]
fn parse_timestamp_str_rejects_zero_and_negative_strings() {
    assert!(parse_timestamp_str("0").is_none());
    assert!(parse_timestamp_str("-5").is_none());
}

#[test]
fn parse_timestamp_str_accepts_timezone_less_datetimes_as_utc() {
    // "2026-06-16T12:00:00" UTC == 1781611200000 ms.
    assert_eq!(
        parse_timestamp_str("2026-06-16T12:00:00"),
        Some(1_781_611_200_000)
    );
    // Space separator and fractional seconds variants.
    assert_eq!(
        parse_timestamp_str("2026-06-16 12:00:00"),
        Some(1_781_611_200_000)
    );
    assert_eq!(
        parse_timestamp_str("2026-06-16T12:00:00.500"),
        Some(1_781_611_200_500)
    );
    // Offset-bearing input still goes through the rfc3339 path unchanged.
    assert_eq!(
        parse_timestamp_str("2026-06-16T12:00:00Z"),
        Some(1_781_611_200_000)
    );
}
