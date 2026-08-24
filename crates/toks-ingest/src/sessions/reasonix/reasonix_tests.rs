use super::*;
use tempfile::NamedTempFile;

#[test]
fn parses_authoritative_stats_with_provider_usage_and_timestamp() {
    let file = NamedTempFile::new().unwrap();
    std::fs::write(
        file.path(),
        concat!(
            "{\"ts\":\"2026-08-04T09:10:11Z\",\"model\":\"opencode/deepseek-v4\",\"prompt\":100,\"completion\":20,\"reasoning\":5,\"cache_hit\":30,\"cache_miss\":70,\"total\":120,\"requests\":1}\n",
            "{\"ts\":\"2026-08-04T09:11:11Z\",\"turn\":true}\n",
        ),
    )
    .unwrap();

    let messages = parse_reasonix_file(file.path());
    assert_eq!(messages.len(), 1);
    let message = &messages[0];
    assert_eq!(message.client, "reasonix");
    assert_eq!(message.provider_id, "opencode");
    assert_eq!(message.model_id, "deepseek-v4");
    assert_eq!(message.tokens.input, 70);
    assert_eq!(message.tokens.output, 15);
    assert_eq!(message.tokens.reasoning, 5);
    assert_eq!(message.tokens.cache_read, 30);
    assert_eq!(message.tokens.cache_write, 0);
    assert_eq!(message.tokens.total(), 120);
    assert_eq!(message.message_count, 1);
    assert_eq!(
        message.timestamp,
        parse_timestamp_value(&serde_json::json!("2026-08-04T09:10:11Z")).unwrap()
    );
}

#[test]
fn skips_turn_markers_malformed_and_zero_usage_records() {
    let file = NamedTempFile::new().unwrap();
    std::fs::write(
        file.path(),
        concat!(
            "not json\n",
            "{\"ts\":\"2026-08-04T09:10:11Z\",\"turn\":true}\n",
            "{\"ts\":\"2026-08-04T09:10:11Z\",\"model\":\"deepseek/test\",\"total\":0}\n",
        ),
    )
    .unwrap();
    assert!(parse_reasonix_file(file.path()).is_empty());
}

#[test]
fn preserves_unknown_model_provider_as_reasonix_only_when_not_inferable() {
    assert_eq!(
        split_model_ref("deepseek/chat"),
        ("deepseek".into(), "chat".into())
    );
    assert_eq!(
        split_model_ref("openrouter/google/gemini-2.5-pro"),
        ("openrouter".into(), "google/gemini-2.5-pro".into())
    );
    assert_eq!(
        split_model_ref("claude-sonnet-4"),
        ("anthropic".into(), "claude-sonnet-4".into())
    );
}

#[test]
fn preserves_explicit_cache_miss_when_it_disagrees_with_prompt_input() {
    let file = NamedTempFile::new().unwrap();
    std::fs::write(
        file.path(),
        "{\"ts\":\"2026-08-04T09:10:11Z\",\"model\":\"deepseek/chat\",\"prompt\":100,\"completion\":20,\"cache_hit\":30,\"cache_miss\":10,\"total\":120}\n",
    )
    .unwrap();

    let messages = parse_reasonix_file(file.path());
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 10);
    assert_eq!(messages[0].tokens.cache_read, 30);
    assert_eq!(messages[0].tokens.total(), 60);
}

#[test]
fn falls_back_to_prompt_minus_cache_hit_when_cache_miss_is_absent() {
    let file = NamedTempFile::new().unwrap();
    std::fs::write(
        file.path(),
        "{\"ts\":\"2026-08-04T09:10:11Z\",\"model\":\"deepseek/chat\",\"prompt\":100,\"completion\":20,\"cache_hit\":30,\"total\":120}\n",
    )
    .unwrap();

    let messages = parse_reasonix_file(file.path());
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 70);
    assert_eq!(messages[0].tokens.total(), 120);
}

#[test]
fn maps_authoritative_request_count_to_bounded_message_count() {
    let file = NamedTempFile::new().unwrap();
    std::fs::write(
        file.path(),
        concat!(
            "{\"ts\":\"2026-08-04T09:10:11Z\",\"model\":\"deepseek/chat\",\"prompt\":1,\"completion\":1,\"total\":2,\"requests\":3}\n",
            "{\"ts\":\"2026-08-04T09:11:11Z\",\"model\":\"deepseek/chat\",\"prompt\":1,\"completion\":1,\"total\":2,\"requests\":0}\n",
            "{\"ts\":\"2026-08-04T09:12:11Z\",\"model\":\"deepseek/chat\",\"prompt\":1,\"completion\":1,\"total\":2,\"requests\":9999999999}\n",
        ),
    )
    .unwrap();

    let messages = parse_reasonix_file(file.path());
    assert_eq!(
        messages
            .iter()
            .map(|message| message.message_count)
            .collect::<Vec<_>>(),
        vec![3, 1, i32::MAX]
    );
}

#[test]
fn preserves_tokenless_request_counts_but_skips_plain_zero_rows() {
    let file = NamedTempFile::new().unwrap();
    std::fs::write(
        file.path(),
        concat!(
            "{\"ts\":\"2026-08-04T09:10:11Z\",\"model\":\"deepseek/chat\",\"total\":0,\"requests\":2}\n",
            "{\"ts\":\"2026-08-04T09:11:11Z\",\"model\":\"deepseek/chat\",\"total\":0}\n",
        ),
    )
    .unwrap();

    let messages = parse_reasonix_file(file.path());
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.total(), 0);
    assert_eq!(messages[0].message_count, 2);
}
