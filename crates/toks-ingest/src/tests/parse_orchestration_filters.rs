use super::{support::*, *};
#[test]
fn test_retain_for_requested_clients_keeps_original_client_matches() {
    let requested: HashSet<&str> = HashSet::from(["opencode"]);
    assert!(retain_for_requested_clients(
        "opencode",
        "gpt-4o",
        "anthropic",
        &requested
    ));
    assert!(!retain_for_requested_clients(
        "claude",
        "gpt-4o",
        "anthropic",
        &requested
    ));
}

#[test]
fn test_retain_for_requested_clients_accepts_synthetic_gateway_traffic() {
    let requested: HashSet<&str> = HashSet::from(["synthetic"]);
    assert!(retain_for_requested_clients(
        "opencode",
        "hf:deepseek-ai/DeepSeek-V3-0324",
        "unknown",
        &requested
    ));
    assert!(retain_for_requested_clients(
        "synthetic",
        "deepseek-v3-0324",
        "synthetic",
        &requested
    ));
    assert!(!retain_for_requested_clients(
        "opencode",
        "gpt-4o",
        "anthropic",
        &requested
    ));
}

#[test]
fn test_retain_for_requested_clients_preserves_kilo_split() {
    let kilocode_only: HashSet<&str> = HashSet::from(["kilocode"]);
    assert!(retain_for_requested_clients(
        "kilocode",
        "gpt-5",
        "openai",
        &kilocode_only
    ));
    assert!(!retain_for_requested_clients(
        "kilo",
        "gpt-5",
        "openai",
        &kilocode_only
    ));

    let kilo_only: HashSet<&str> = HashSet::from(["kilo"]);
    assert!(retain_for_requested_clients(
        "kilo", "gpt-5", "openai", &kilo_only
    ));
    assert!(!retain_for_requested_clients(
        "kilocode", "gpt-5", "openai", &kilo_only
    ));
}

#[test]
fn test_parse_all_messages_with_pricing_keeps_gateway_message_under_synthetic_filter() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let message_dir = temp_dir
        .path()
        .join(".local/share/opencode/storage/message/project-1");
    std::fs::create_dir_all(&message_dir).unwrap();
    std::fs::write(
            message_dir.join("msg_001.json"),
            r#"{"id":"msg-1","sessionID":"session-1","role":"assistant","modelID":"hf:deepseek-ai/DeepSeek-V3-0324","providerID":"unknown","cost":0,"tokens":{"input":10,"output":5,"reasoning":0,"cache":{"read":0,"write":0}},"time":{"created":1733011200000}}"#,
        )
        .unwrap();

    let pricing = pricing::PricingService::new(HashMap::new(), HashMap::new());
    let messages = parse_all_messages_with_pricing(
        temp_dir.path().to_str().unwrap(),
        &["synthetic".to_string()],
        Some(&pricing),
    );

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].client, "opencode");
    assert_eq!(messages[0].model_id, "deepseek-v3-0324");
    assert_eq!(messages[0].provider_id, "synthetic");
}

#[test]
fn test_parse_all_messages_fireworks_provider_kept_under_synthetic_only_filter() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let message_dir = temp_dir
        .path()
        .join(".local/share/opencode/storage/message/project-1");
    std::fs::create_dir_all(&message_dir).unwrap();
    std::fs::write(
            message_dir.join("msg_001.json"),
            r#"{"id":"msg-1","sessionID":"session-1","role":"assistant","modelID":"accounts/fireworks/models/deepseek-v3-0324","providerID":"fireworks","cost":0.1,"tokens":{"input":10,"output":5,"reasoning":0,"cache":{"read":0,"write":0}},"time":{"created":1733011200000}}"#,
        )
        .unwrap();

    let pricing = pricing::PricingService::new(HashMap::new(), HashMap::new());
    let messages = parse_all_messages_with_pricing(
        temp_dir.path().to_str().unwrap(),
        &["synthetic".to_string()],
        Some(&pricing),
    );

    assert_eq!(
        messages.len(),
        1,
        "fireworks gateway message must not be dropped when filtering for synthetic"
    );
    assert_eq!(messages[0].client, "opencode");
    assert_eq!(messages[0].model_id, "deepseek-v3-0324");
    // Provider is canonicalized by the opencode parser (fireworks -> fireworks_ai).
    assert_eq!(messages[0].provider_id, "fireworks_ai");
}

#[test]
fn test_retain_for_requested_clients_gjc_superset_of_9router() {
    let gjc_requested: HashSet<&str> = HashSet::from(["gjc"]);
    // Bridge messages carry client="9router"; requesting "gjc" retains
    // them (9router data IS gjc-format, so gjc is a superset request).
    assert!(retain_for_requested_clients(
        "9router",
        "deepseek-ai/deepseek-v4-flash",
        "nvidia",
        &gjc_requested
    ));
    // --client 9router retains bridge-stamped messages…
    let ninerouter_requested: HashSet<&str> = HashSet::from(["9router"]);
    assert!(retain_for_requested_clients(
        "9router",
        "deepseek-ai/deepseek-v4-flash",
        "nvidia",
        &ninerouter_requested
    ));
    // …but must NOT retain native gjc messages: the alias is one-way
    // (gjc is the superset request, 9router is the narrow one).
    assert!(!retain_for_requested_clients(
        "gjc",
        "claude-sonnet-4",
        "anthropic",
        &ninerouter_requested
    ));
    // Unrelated clients still filtered out.
    assert!(!retain_for_requested_clients(
        "claude",
        "gpt-4o",
        "openai",
        &gjc_requested
    ));
}
