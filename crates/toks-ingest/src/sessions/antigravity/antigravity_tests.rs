use super::*;

#[test]
fn parse_usage_row_with_meta_fallback() {
    let input = r#"{"type":"session_meta","sessionId":"abc","modelId":"claude-sonnet-4.6"}
{"type":"usage","sessionId":"abc","timestamp":1711200000000,"input":12,"output":4,"cacheRead":2,"cacheWrite":0,"reasoning":1,"responseId":"resp-1"}
"#;

    let path = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(path.path(), input).unwrap();

    let messages = parse_antigravity_file(path.path());
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].client, "antigravity");
    assert_eq!(messages[0].model_id, "claude-sonnet-4-6");
    assert_eq!(messages[0].tokens.input, 12);
    assert_eq!(messages[0].tokens.reasoning, 1);
    assert_eq!(messages[0].dedup_key.as_deref(), Some("resp-1"));
}

#[test]
fn parse_usage_row_resolves_placeholder_model_alias() {
    let input = r#"{"type":"usage","sessionId":"abc","modelId":"MODEL_PLACEHOLDER_M26","timestamp":1711200000000,"input":12,"output":4,"cacheRead":2,"cacheWrite":0,"reasoning":1}
"#;

    let path = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(path.path(), input).unwrap();

    let messages = parse_antigravity_file(path.path());
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "claude-opus-4-6");
    assert_eq!(messages[0].provider_id, "anthropic");
}

#[test]
fn parse_usage_row_resolves_current_placeholder_models() {
    let input = r#"{"type":"usage","sessionId":"abc","modelId":"model_placeholder_m84","timestamp":1711200000000,"input":12,"output":4,"cacheRead":2,"cacheWrite":0,"reasoning":1}
{"type":"usage","sessionId":"abc","modelId":"model_placeholder_m16","timestamp":1711200000001,"input":8,"output":3,"cacheRead":0,"cacheWrite":0,"reasoning":0}
"#;

    let path = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(path.path(), input).unwrap();

    let messages = parse_antigravity_file(path.path());
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].model_id, "gemini-3-flash-preview");
    assert_eq!(messages[0].provider_id, "google");
    assert_eq!(messages[1].model_id, "gemini-3.1-pro");
    assert_eq!(messages[1].provider_id, "google");
}
