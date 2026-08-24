use super::*;

#[test]
fn test_parse_model_config_valid() {
    let json = r#"{"model_name":"claude-sonnet-4-20250514","context_limit":200000}"#;
    assert_eq!(
        parse_model_config(json),
        Some("claude-sonnet-4-20250514".to_string())
    );
}

#[test]
fn test_parse_model_config_empty_name() {
    let json = r#"{"model_name":"  ","context_limit":200000}"#;
    assert_eq!(parse_model_config(json), None);
}

#[test]
fn test_parse_model_config_invalid_json() {
    assert_eq!(parse_model_config("not json"), None);
}

#[test]
fn test_timestamp_secs_to_ms() {
    assert_eq!(timestamp_secs_to_ms(1_700_000_000.0), 1_700_000_000_000);
    assert_eq!(timestamp_secs_to_ms(1_700_000_000_000.0), 1_700_000_000_000);
}

#[test]
fn test_parse_created_at_rfc3339() {
    let ts = parse_created_at("2026-04-14T16:18:53Z");
    assert!(ts > 0.0);
}

#[test]
fn test_parse_created_at_sqlite_timestamp() {
    let ts = parse_created_at("2026-04-14 16:18:53");
    assert!(ts > 0.0);
    let expected = chrono::NaiveDateTime::parse_from_str("2026-04-14 16:18:53", "%Y-%m-%d %H:%M:%S")
        .unwrap()
        .and_utc()
        .timestamp_millis() as f64;
    assert_eq!(ts, expected);
}

#[test]
fn test_parse_created_at_date_only() {
    let ts = parse_created_at("2026-04-14");
    assert!(ts > 0.0);
}

#[test]
fn test_parse_created_at_invalid() {
    assert_eq!(parse_created_at("not a date"), 0.0);
}
