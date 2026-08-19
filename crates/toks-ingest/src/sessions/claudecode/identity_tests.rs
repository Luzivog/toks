use super::*;
use crate::{DurableIdentityScheme, IdentityStrength};

fn parse(content: &str) -> Vec<UnifiedMessage> {
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), content).unwrap();
    parse_claude_file(file.path())
}

#[test]
fn assistant_provider_response_receives_strong_identity() {
    let messages = parse(
        r#"{"type":"assistant","timestamp":"2024-12-01T10:00:00Z","requestId":"req_001","message":{"id":"msg_001","model":"claude-3-5-sonnet","usage":{"input_tokens":1000,"output_tokens":500,"cache_read_input_tokens":200,"cache_creation_input_tokens":100}}}"#,
    );
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 1000);
    let identity = messages[0].durable_identity.as_ref().unwrap();
    assert_eq!(
        identity.scheme,
        DurableIdentityScheme::ClaudeProviderResponse
    );
    assert_eq!(identity.value, "msg_001:req_001");
    assert_eq!(identity.strength, IdentityStrength::Strong);
}

#[test]
fn path_scoped_tool_result_has_no_durable_identity() {
    let messages = parse(
        r#"{"type":"user","timestamp":"2026-05-27T10:00:00Z","message":{"model":"sonnet","content":[{"type":"tool_result","tool_use_id":"toolu_1","tool_output":{"input_tokens":7,"output":"tool output"}}]}}"#,
    );
    assert_eq!(messages.len(), 1);
    assert!(messages[0].durable_identity.is_none());
}
