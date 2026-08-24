use super::{support::*, *};
#[test]
fn test_cursor_parse_path_reprices_zero_cost_composer_1_5_rows() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let cursor_cache_dir = temp_dir.path().join(".config/toks/cursor-cache");
    std::fs::create_dir_all(&cursor_cache_dir).unwrap();

    let csv = r#"Date,Kind,Model,Max Mode,Input (w/ Cache Write),Input (w/o Cache Write),Cache Read,Output Tokens,Total Tokens,Cost
"2026-03-04T12:00:00.000Z","Included","Composer 1.5","No","1200","1000","5000","2000","8000","0""#;
    std::fs::write(cursor_cache_dir.join("usage.csv"), csv).unwrap();

    let pricing = pricing::PricingService::new(HashMap::new(), HashMap::new());
    let messages = parse_all_messages_with_pricing(
        temp_dir.path().to_str().unwrap(),
        &["cursor".to_string()],
        Some(&pricing),
    );

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].client, "cursor");
    assert_eq!(messages[0].model_id, "Composer 1.5");
    assert!(messages[0].cost > 0.0);
}

/// MiMo Code records carry an authoritative per-message cost. The micode
/// lane must NOT reprice a record that already has a cost, even when the
/// model has a market price that would compute a different (non-zero) value.
/// This must hold on the first parse AND on a subsequent cache hit, since
/// the previous bug repriced and persisted the inflated cost to the cache.
#[test]
#[serial_test::serial]
fn test_micode_authoritative_cost_is_not_repriced_on_first_parse_or_cache_hit() {
    let cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let _cache_env = redirect_cache_home(cache_home.path());

    {
        let micode_dir = source_home.path().join(".local/share/mimocode");
        std::fs::create_dir_all(&micode_dir).unwrap();
        let db_path = micode_dir.join("mimocode.db");

        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE message (
                    id TEXT PRIMARY KEY,
                    session_id TEXT NOT NULL,
                    data TEXT NOT NULL
                );",
        )
        .unwrap();
        // Authoritative cost 0.05 with 1000 input / 500 output tokens.
        let data_json = r#"{
                "role": "assistant",
                "modelID": "mimo-v2.5-pro",
                "providerID": "mimo",
                "cost": 0.05,
                "tokens": { "input": 1000, "output": 500, "reasoning": 0, "cache": { "read": 0, "write": 0 } },
                "time": { "created": 1700000000000.0 }
            }"#;
        conn.execute(
            "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
            rusqlite::params!["msg_auth_cost", "ses_1", data_json],
        )
        .unwrap();
        drop(conn);

        // Pricing that WOULD reprice mimo-v2.5-pro to a different non-zero
        // value (1000 * 0.001 + 500 * 0.002 = 2.0) if the guard were absent.
        let mut litellm = HashMap::new();
        litellm.insert(
            "mimo-v2.5-pro".into(),
            pricing::ModelPricing {
                input_cost_per_token: Some(0.001),
                output_cost_per_token: Some(0.002),
                ..Default::default()
            },
        );
        let pricing = pricing::PricingService::new(litellm, HashMap::new());

        let first = parse_all_messages_with_pricing(
            source_home.path().to_str().unwrap(),
            &["micode".to_string()],
            Some(&pricing),
        );
        assert_eq!(first.len(), 1);
        assert!(
            (first[0].cost - 0.05).abs() < 1e-9,
            "authoritative cost must survive the first parse, got {}",
            first[0].cost
        );

        // Second run hits the source cache; the persisted entry must still
        // carry the authoritative cost rather than a repriced value.
        let second = parse_all_messages_with_pricing(
            source_home.path().to_str().unwrap(),
            &["micode".to_string()],
            Some(&pricing),
        );
        assert_eq!(second.len(), 1);
        assert!(
            (second[0].cost - 0.05).abs() < 1e-9,
            "authoritative cost must survive the cache hit, got {}",
            second[0].cost
        );
    }
}

#[test]
#[serial_test::serial]
fn test_micode_cross_database_dedup_prefers_explicit_zero_cost() {
    let cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let _cache_env = redirect_cache_home(cache_home.path());

    {
        let micode_dir = source_home.path().join(".local/share/mimocode");
        std::fs::create_dir_all(&micode_dir).unwrap();
        let without_cost = r#"{
                "id": "shared-message",
                "role": "assistant",
                "modelID": "unknown-model",
                "providerID": "mimo",
                "tokens": { "input": 10, "output": 5 },
                "time": { "created": 1700000000000.0 }
            }"#;
        let with_zero_cost = r#"{
                "id": "shared-message",
                "role": "assistant",
                "modelID": "unknown-model",
                "providerID": "mimo",
                "cost": 0,
                "tokens": { "input": 10, "output": 5 },
                "time": { "created": 1700000000000.0 }
            }"#;
        for (name, data) in [
            ("mimocode-alpha.db", without_cost),
            ("mimocode-beta.db", with_zero_cost),
        ] {
            let db_path = micode_dir.join(name);
            let conn = rusqlite::Connection::open(db_path).unwrap();
            conn.execute_batch(
                "CREATE TABLE message (
                        id TEXT PRIMARY KEY,
                        session_id TEXT NOT NULL,
                        data TEXT NOT NULL
                    );",
            )
            .unwrap();
            conn.execute(
                "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
                rusqlite::params![name, "session", data],
            )
            .unwrap();
        }

        let pricing = pricing::PricingService::new(HashMap::new(), HashMap::new());
        let messages = parse_all_messages_with_pricing(
            source_home.path().to_str().unwrap(),
            &["micode".to_string()],
            Some(&pricing),
        );

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].cost, 0.0);
        assert!(messages[0].has_authoritative_cost());
        assert!(validate_priced_messages(&messages, Some(&pricing)).is_ok());
    }
}

#[test]
#[serial_test::serial]
fn test_parse_all_messages_with_pricing_prefers_grok_unified_log() {
    let cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let _cache_env = redirect_cache_home(cache_home.path());

    {
        let session_dir = source_home
            .path()
            .join(".grok/sessions/%2Ftmp%2Fproject/session-1");
        std::fs::create_dir_all(&session_dir).unwrap();
        std::fs::write(
                session_dir.join("updates.jsonl"),
                r#"{"method":"session/update","params":{"sessionId":"session-1","_meta":{"totalTokens":999,"agentTimestampMs":1700000000000}}}"#,
            )
            .unwrap();

        let logs_dir = source_home.path().join(".grok/logs");
        std::fs::create_dir_all(&logs_dir).unwrap();
        std::fs::write(
                logs_dir.join("unified.jsonl"),
                r#"{"ts":"2023-11-14T22:13:20Z","pid":7,"sid":"session-1","msg":"shell.turn.inference_done","ctx":{"loop_index":1,"prompt_tokens":100,"cached_prompt_tokens":60,"completion_tokens":25,"reasoning_tokens":5}}"#,
            )
            .unwrap();

        let messages = parse_all_messages_with_pricing(
            source_home.path().to_str().unwrap(),
            &["grok".to_string()],
            None,
        );

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].tokens.input, 40);
        assert_eq!(messages[0].tokens.cache_read, 60);
        assert_eq!(messages[0].tokens.output, 20);
        assert_eq!(messages[0].tokens.reasoning, 5);
        assert_eq!(messages[0].tokens.total(), 125);
    }
}

#[test]
#[serial_test::serial]
fn test_parse_all_messages_reprices_grok_after_legacy_model_attribution() {
    let cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let _cache_env = redirect_cache_home(cache_home.path());

    {
        let session_dir = source_home
            .path()
            .join(".grok/sessions/%2Ftmp%2Fproject/session-1");
        std::fs::create_dir_all(&session_dir).unwrap();
        std::fs::write(
                session_dir.join("updates.jsonl"),
                r#"{"method":"session/update","params":{"sessionId":"session-1","update":{"sessionUpdate":"user_message_chunk","_meta":{"modelId":"grok-code"}},"_meta":{"agentTimestampMs":1700000000000}}}
{"method":"session/update","params":{"sessionId":"session-1","update":{"sessionUpdate":"agent_message_chunk"},"_meta":{"totalTokens":999,"agentTimestampMs":1700000000000}}}"#,
            )
            .unwrap();

        let logs_dir = source_home.path().join(".grok/logs");
        std::fs::create_dir_all(&logs_dir).unwrap();
        std::fs::write(
                logs_dir.join("unified.jsonl"),
                r#"{"ts":"2023-11-14T22:13:20Z","pid":7,"sid":"session-1","msg":"shell.turn.inference_done","ctx":{"loop_index":1,"prompt_tokens":100,"cached_prompt_tokens":60,"completion_tokens":25,"reasoning_tokens":5}}"#,
            )
            .unwrap();

        let mut litellm = HashMap::new();
        litellm.insert(
            "grok-code".to_string(),
            pricing::ModelPricing {
                input_cost_per_token: Some(0.001),
                output_cost_per_token: Some(0.002),
                ..Default::default()
            },
        );
        let pricing = pricing::PricingService::new(litellm, HashMap::new());

        let first = parse_all_messages_with_pricing_with_env_strategy(
            source_home.path().to_str().unwrap(),
            &["grok".to_string()],
            Some(&pricing),
            false,
            &scanner::ScannerSettings::default(),
        );
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].model_id, "grok-code");
        assert!(first[0].cost > 0.0);

        let second = parse_all_messages_with_pricing_with_env_strategy(
            source_home.path().to_str().unwrap(),
            &["grok".to_string()],
            Some(&pricing),
            false,
            &scanner::ScannerSettings::default(),
        );
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].model_id, "grok-code");
        assert!(second[0].cost > 0.0);
    }
}

#[test]
#[serial_test::serial]
fn test_parse_all_messages_keeps_conflicted_grok_scoped_model_change_unpriced_cold_and_warm() {
    let cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let mut env = paths::test_env::EnvGuard::capture(&["HOME"]);
    env.set("HOME", cache_home.path());

    {
        let logs_dir = source_home.path().join(".grok/logs");
        std::fs::create_dir_all(&logs_dir).unwrap();
        std::fs::write(
                logs_dir.join("unified.jsonl"),
                r#"{"ts":"2026-07-31T00:00:01Z","pid":19,"msg":"subagent spawn credentials","ctx":{"subagent_id":"child","effective_model":"grok-4.8"}}
{"ts":"2026-07-31T00:00:02Z","pid":19,"sid":"child","msg":"model changed","ctx":{"model":"grok-code"}}
{"ts":"2026-07-31T00:00:03Z","pid":19,"msg":"subagent failed","ctx":{"subagent_id":"child","effective_model":"grok-4.9"}}
{"ts":"2026-07-31T00:00:04Z","pid":19,"sid":"child","msg":"shell.turn.inference_done","ctx":{"loop_index":1,"prompt_tokens":100,"cached_prompt_tokens":60,"completion_tokens":25,"reasoning_tokens":5}}"#,
            )
            .unwrap();

        let mut litellm = HashMap::new();
        litellm.insert(
            "grok-code".to_string(),
            pricing::ModelPricing {
                input_cost_per_token: Some(0.001),
                output_cost_per_token: Some(0.002),
                ..Default::default()
            },
        );
        let pricing = pricing::PricingService::new(litellm, HashMap::new());

        for scan in ["cold", "warm"] {
            let messages = parse_all_messages_with_pricing_with_env_strategy(
                source_home.path().to_str().unwrap(),
                &["grok".to_string()],
                Some(&pricing),
                false,
                &scanner::ScannerSettings::default(),
            );
            assert_eq!(messages.len(), 1, "{scan} scan message count");
            assert_eq!(messages[0].model_id, "grok-unknown", "{scan} scan");
            assert!(messages[0].model_attribution_conflicted, "{scan} scan");
            assert_eq!(messages[0].cost, 0.0, "{scan} scan");
        }
    }
}
