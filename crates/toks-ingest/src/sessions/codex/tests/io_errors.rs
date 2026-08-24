use super::*;

struct FailAfterFirstLine {
    inner: Cursor<Vec<u8>>,
    first_line_len: u64,
}

impl FailAfterFirstLine {
    fn new(contents: &str) -> Self {
        Self {
            inner: Cursor::new(contents.as_bytes().to_vec()),
            first_line_len: contents.find('\n').map_or(0, |index| index as u64 + 1),
        }
    }
}

impl std::io::Read for FailAfterFirstLine {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.inner.read(buf)
    }
}

impl BufRead for FailAfterFirstLine {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        if self.inner.position() >= self.first_line_len {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "synthetic line read failure",
            ));
        }
        self.inner.fill_buf()
    }

    fn consume(&mut self, amt: usize) {
        self.inner.consume(amt);
    }
}

#[test]
fn test_parse_reader_marks_failure_on_line_read_error() {
    let reader = FailAfterFirstLine::new(concat!(
        r#"{"type":"turn_context","payload":{"model":"gpt-5.4"}}"#,
        "\n",
        r#"{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3},"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3}}}}"#,
        "\n"
    ));

    let parsed = parse_codex_reader(reader, "session", 0, 0, CodexParseState::default());

    assert!(!parsed.parse_succeeded);
    assert!(parsed.messages.is_empty());
}

#[test]
fn test_parse_file_returns_empty_on_invalid_utf8_line_error() {
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(
        concat!(
            r#"{"type":"turn_context","payload":{"model":"gpt-5.4"}}"#,
            "\n"
        )
        .as_bytes(),
    )
    .unwrap();
    file.write_all(&[0xff, b'\n']).unwrap();
    file.flush().unwrap();

    let messages = parse_codex_file(file.path());
    assert!(messages.is_empty());

    let incremental = parse_codex_file_incremental(file.path(), 0, CodexParseState::default());
    assert!(!incremental.parse_succeeded);
}

#[test]
fn test_parse_file_preserves_valid_messages_after_late_invalid_utf8_line_error() {
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(
            concat!(
                r#"{"type":"turn_context","payload":{"model":"gpt-5.4"}}"#,
                "\n",
                r#"{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3},"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3}}}}"#,
                "\n"
            )
            .as_bytes(),
        )
        .unwrap();
    file.write_all(&[0xff, b'\n']).unwrap();
    file.flush().unwrap();

    let messages = parse_codex_file(file.path());
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "gpt-5.4");
    assert_eq!(messages[0].tokens.input, 8);
    assert_eq!(messages[0].tokens.output, 3);
    assert_eq!(messages[0].tokens.cache_read, 2);

    let incremental = parse_codex_file_incremental(file.path(), 0, CodexParseState::default());
    assert!(!incremental.parse_succeeded);
    assert_eq!(incremental.messages.len(), 1);
}
