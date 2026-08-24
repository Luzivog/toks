use std::io::Write;

use tempfile::NamedTempFile;

use super::super::parse_claude_file;

#[test]
fn parser_keeps_valid_rows_after_invalid_utf8() {
    let first = br#"{"type":"assistant","timestamp":"2024-12-01T10:00:00.000Z","requestId":"req_001","message":{"id":"msg_001","model":"claude-3-5-sonnet","usage":{"input_tokens":100,"output_tokens":50}}}
"#;
    let second = br#"{"type":"assistant","timestamp":"2024-12-01T10:00:02.000Z","requestId":"req_002","message":{"id":"msg_002","model":"claude-3-5-sonnet","usage":{"input_tokens":200,"output_tokens":100}}}
"#;
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(first).unwrap();
    file.write_all(b"{\"invalid\":\"").unwrap();
    file.write_all(&[0xff, b'\n']).unwrap();
    file.write_all(second).unwrap();
    file.flush().unwrap();

    let messages = parse_claude_file(file.path());

    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].dedup_key.as_deref(), Some("msg_001:req_001"));
    assert_eq!(messages[1].dedup_key.as_deref(), Some("msg_002:req_002"));
}
