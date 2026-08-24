use super::{support::*, *};
#[test]
#[serial_test::serial]
fn test_source_cache_does_not_reuse_priced_cost_without_pricing_service() {
    let temp_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let _cache_env = redirect_cache_home(temp_home.path());
    {
        let cursor_cache_dir = source_home.path().join(".config/toks/cursor-cache");
        std::fs::create_dir_all(&cursor_cache_dir).unwrap();

        let csv = r#"Date,Kind,Model,Max Mode,Input (w/ Cache Write),Input (w/o Cache Write),Cache Read,Output Tokens,Total Tokens,Cost
"2026-03-04T12:00:00.000Z","Included","Composer 1.5","No","1200","1000","5000","2000","8000","0""#;
        std::fs::write(cursor_cache_dir.join("usage.csv"), csv).unwrap();

        let mut litellm = HashMap::new();
        litellm.insert(
            "Composer 1.5".into(),
            pricing::ModelPricing {
                input_cost_per_token: Some(0.001),
                output_cost_per_token: Some(0.002),
                cache_read_input_token_cost: Some(0.0005),
                ..Default::default()
            },
        );
        let pricing = pricing::PricingService::new(litellm, HashMap::new());

        let repriced_messages = parse_all_messages_with_pricing(
            source_home.path().to_str().unwrap(),
            &["cursor".to_string()],
            Some(&pricing),
        );
        assert_eq!(repriced_messages.len(), 1);
        assert!(repriced_messages[0].cost > 0.0);

        let cached_messages = parse_all_messages_with_pricing(
            source_home.path().to_str().unwrap(),
            &["cursor".to_string()],
            None,
        );

        assert_eq!(cached_messages.len(), 1);
        assert_eq!(cached_messages[0].cost, 0.0);
    }
}

#[test]
fn test_opencode_embedded_cost_survives_repricing_while_missing_cost_reprices() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let message_dir = temp_dir
        .path()
        .join(".local/share/opencode/storage/message/project-1");
    std::fs::create_dir_all(&message_dir).unwrap();
    std::fs::write(
            message_dir.join("msg_reported.json"),
            r#"{"id":"msg-reported","sessionID":"session-1","role":"assistant","modelID":"gpt-4o","providerID":"openai","cost":0.05,"tokens":{"input":10,"output":5,"reasoning":0,"cache":{"read":0,"write":0}},"time":{"created":1733011200000}}"#,
        )
        .unwrap();
    std::fs::write(
            message_dir.join("msg_missing.json"),
            r#"{"id":"msg-missing","sessionID":"session-1","role":"assistant","modelID":"gpt-4o","providerID":"openai","tokens":{"input":10,"output":5,"reasoning":0,"cache":{"read":0,"write":0}},"time":{"created":1733011201000}}"#,
        )
        .unwrap();

    let mut litellm = HashMap::new();
    litellm.insert(
        "openai/gpt-4o".to_string(),
        pricing::ModelPricing {
            input_cost_per_token: Some(0.01),
            output_cost_per_token: Some(0.02),
            ..Default::default()
        },
    );
    let pricing = pricing::PricingService::new(litellm, HashMap::new());
    let messages = parse_all_messages_with_pricing_with_env_strategy(
        temp_dir.path().to_str().unwrap(),
        &["opencode".to_string()],
        Some(&pricing),
        false,
        &scanner::ScannerSettings::default(),
    );

    let embedded = messages
        .iter()
        .find(|message| message.dedup_key.as_deref() == Some("msg-reported"))
        .expect("embedded-cost message should parse");
    let missing = messages
        .iter()
        .find(|message| message.dedup_key.as_deref() == Some("msg-missing"))
        .expect("missing-cost message should parse");
    assert_eq!(
            embedded.cost, 0.05,
            "OpenCode computes cost at request time; the embedded value must not be overwritten by LiteLLM repricing"
        );
    assert_eq!(embedded.cost_source, crate::CostSource::ProviderReported);
    assert_eq!(missing.cost, 0.2);
    assert_eq!(missing.cost_source, crate::CostSource::Estimated);
}

#[test]
fn test_gjc_explicit_zero_cost_is_preserved_while_absent_cost_reprices() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let session_dir = temp_dir.path().join(".gjc/agent/sessions/project-1");
    std::fs::create_dir_all(&session_dir).unwrap();
    std::fs::write(
            session_dir.join("session.jsonl"),
            r#"{"type":"session","id":"gjc_ses_cost","cwd":"/work/project-1"}
{"type":"message","id":"msg_zero","message":{"role":"assistant","model":"gpt-4o","provider":"openai","timestamp":1733011200000,"usage":{"input":10,"output":5,"cost":{"total":0.0}}}}
{"type":"message","id":"msg_absent","message":{"role":"assistant","model":"gpt-4o","provider":"openai","timestamp":1733011201000,"usage":{"input":10,"output":5}}}"#,
        )
        .unwrap();

    let mut litellm = HashMap::new();
    litellm.insert(
        "openai/gpt-4o".to_string(),
        pricing::ModelPricing {
            input_cost_per_token: Some(0.01),
            output_cost_per_token: Some(0.02),
            ..Default::default()
        },
    );
    let pricing = pricing::PricingService::new(litellm, HashMap::new());
    let messages = parse_all_messages_with_pricing_with_env_strategy(
        temp_dir.path().to_str().unwrap(),
        &["gjc".to_string()],
        Some(&pricing),
        false,
        &scanner::ScannerSettings::default(),
    );

    let explicit_zero = messages
        .iter()
        .find(|message| message.dedup_key.as_deref() == Some("gjc_ses_cost:msg_zero"))
        .expect("explicit-zero message should parse");
    let absent = messages
        .iter()
        .find(|message| message.dedup_key.as_deref() == Some("gjc_ses_cost:msg_absent"))
        .expect("absent-cost message should parse");
    assert_eq!(explicit_zero.cost, 0.0);
    assert_eq!(absent.cost, 0.2);
}

#[test]
fn test_gjc_idless_replay_dedup_stable_across_ordinal_shift() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let session_dir = temp_dir.path().join(".gjc/agent/sessions/project-1");
    let child_dir = session_dir.join("session");
    std::fs::create_dir_all(&child_dir).unwrap();
    let assistant_line = r#"{"type":"message","message":{"role":"assistant","model":"gpt-4o","provider":"openai","timestamp":1733011200000,"usage":{"input":10,"output":5,"cost":{"total":0.03}}}}"#;
    std::fs::write(
        session_dir.join("session.jsonl"),
        format!(
            "{}\n{}\n",
            r#"{"type":"session","id":"gjc_ses_replay_idless","cwd":"/work/project-1"}"#,
            assistant_line
        ),
    )
    .unwrap();
    std::fs::write(
        child_dir.join("1-replay.jsonl"),
        format!(
            "{}\n{}\n{}\n",
            r#"{"type":"session","id":"gjc_ses_replay_idless","cwd":"/work/project-1"}"#,
            r#"{"type":"service_tier_change","tier":"pro"}"#,
            assistant_line
        ),
    )
    .unwrap();

    let messages = parse_all_messages_with_pricing_with_env_strategy(
        temp_dir.path().to_str().unwrap(),
        &["gjc".to_string()],
        None,
        false,
        &scanner::ScannerSettings::default(),
    );

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].cost, 0.03);
}
