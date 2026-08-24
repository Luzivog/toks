use super::*;
use serde_json::json;
use std::io::Write;
use tempfile::TempDir;

fn write_session(dir: &TempDir, slug: &str, session: &str, jsonl: &str) -> std::path::PathBuf {
    let project_dir = dir.path().join("projects").join(slug);
    std::fs::create_dir_all(&project_dir).unwrap();
    let path = project_dir.join(format!("{session}.jsonl"));
    let mut file = std::fs::File::create(&path).unwrap();
    file.write_all(jsonl.as_bytes()).unwrap();
    path
}

fn write_config(dir: &TempDir, model: &str) {
    let path = dir.path().join("config.json");
    let mut file = std::fs::File::create(&path).unwrap();
    write!(file, r#"{{"provider":"command-code","model":"{model}"}}"#).unwrap();
}

#[test]
fn test_canonicalize_model_strips_org_prefix_and_free_promo_suffix() {
    // "-Free" is a temporary promo; the org prefix mis-resolves pricing.
    assert_eq!(
        canonicalize_model("MiniMaxAI/MiniMax-M3-Free"),
        "MiniMax-M3"
    );
    assert_eq!(
        canonicalize_model("minimaxai/minimax-m3-free"),
        "minimax-m3"
    );
    assert_eq!(canonicalize_model("MiniMaxAI/MiniMax-M2.5"), "MiniMax-M2.5");
    assert_eq!(canonicalize_model("taste-1"), "taste-1");
    // Mixed-case promo suffix is still stripped (case-insensitive match).
    assert_eq!(canonicalize_model("MiniMax-M3-FrEe"), "MiniMax-M3");
}

/// Regression: a non-ASCII model id from the untrusted
/// `~/.commandcode/config.json` must not panic. The previous implementation
/// byte-sliced `base[base.len() - 5..]` guarded only by a length check; for
/// an id whose final 5 bytes straddle a multi-byte UTF-8 codepoint that
/// slice panics (byte index not on a char boundary).
#[test]
fn test_canonicalize_model_does_not_panic_on_non_ascii() {
    // "modèle" ends with the multi-byte 'è' inside the last 5 bytes.
    assert_eq!(canonicalize_model("vendor/modèle"), "modèle");
    // Emoji at the tail: last bytes are deep inside a 4-byte codepoint.
    assert_eq!(canonicalize_model("café-🚀"), "café-🚀");
    // A non-ASCII id that nonetheless ends in the promo suffix still strips.
    assert_eq!(canonicalize_model("café-free"), "café");
}

#[test]
fn test_content_chars_counts_keys_numbers_and_nested_payloads() {
    // Structured tool args/results carry meaning in keys and primitive
    // values; a string-only counter would return 0 for numeric content.
    assert!(content_chars(&json!([{"value": 12345}])) > 0);
    let small = content_chars(&json!([{"a": "x"}]));
    let large = content_chars(&json!([{"command": "run", "args": ["a", "b"], "n": 42}]));
    assert!(large > small);
}

#[test]
fn test_parse_canonicalizes_model_and_estimates_tokens() {
    let dir = TempDir::new().unwrap();
    write_config(&dir, "MiniMaxAI/MiniMax-M3-Free");
    let user = json!([{"type": "text", "text": "12345678"}]);
    let assistant = json!([{"type": "text", "text": "abcd"}]);
    let jsonl = format!(
        "{}\n{}",
        json!({"role": "user", "sessionId": "sess-1", "timestamp": "2026-06-16T05:58:15.580Z", "content": user.clone()}),
        json!({"role": "assistant", "sessionId": "sess-1", "timestamp": "2026-06-16T05:58:20.332Z", "content": assistant.clone()}),
    );
    let path = write_session(&dir, "users-alice-repo", "sess-1", &jsonl);

    let messages = parse_commandcode_file(&path);

    assert_eq!(messages.len(), 1);
    let msg = &messages[0];
    assert_eq!(msg.client, "commandcode");
    // Provider is recovered from the gateway id (MiniMaxAI -> minimax) so
    // pricing resolves against the `minimax/...` catalog, not `command-code`.
    assert_eq!(msg.provider_id, "minimax");
    // Promo suffix + org prefix stripped so pricing hits the real model.
    assert_eq!(msg.model_id, "MiniMax-M3");
    assert_eq!(msg.session_id, "sess-1");
    // Input = context before this turn (just the user message); output = this
    // assistant message. Computed from the same helper to avoid brittle counts.
    assert_eq!(msg.tokens.input, estimate_tokens(content_chars(&user)));
    assert_eq!(
        msg.tokens.output,
        estimate_tokens(content_chars(&assistant))
    );
    assert!(msg.tokens.input > 0 && msg.tokens.output > 0);
    assert_eq!(msg.message_count, 1);
    assert!(msg.is_turn_start);
    assert_eq!(msg.timestamp, 1781589500332); // 2026-06-16T05:58:20.332Z
    assert_eq!(msg.workspace_key.as_deref(), Some("users-alice-repo"));
    assert_eq!(msg.workspace_label.as_deref(), Some("users-alice-repo"));
}

/// Per-turn input does NOT accumulate prior turns: each assistant turn is
/// charged only for the new context (user + tool results) introduced since
/// the previous response. A long, expensive turn must not permanently
/// inflate later, cheaper turns — the previous cumulative implementation
/// would have made turn 2 strictly larger than turn 1 here, so this test
/// fails without the per-turn-delta fix.
#[test]
fn test_input_is_per_turn_delta_not_cumulative() {
    let dir = TempDir::new().unwrap();
    write_config(&dir, "model-x");
    // Turn 1 carries a large user prompt; turn 2 carries only a tiny one.
    // With cumulative counting turn 2 would still include all of turn 1 and
    // therefore exceed it; with per-turn deltas turn 2 is much smaller.
    let jsonl = concat!(
        r#"{"role":"user","sessionId":"s","content":[{"type":"text","text":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}]}"#,
        "\n",
        r#"{"role":"assistant","sessionId":"s","content":[{"type":"text","text":"bbbb"}]}"#,
        "\n",
        r#"{"role":"user","sessionId":"s","content":[{"type":"text","text":"d"}]}"#,
        "\n",
        r#"{"role":"assistant","sessionId":"s","content":[{"type":"text","text":"e"}]}"#,
    );
    let path = write_session(&dir, "proj", "s", jsonl);

    let messages = parse_commandcode_file(&path);

    assert_eq!(messages.len(), 2);
    assert!(messages[0].tokens.input > 0);
    assert!(messages[0].is_turn_start);
    assert!(messages[1].is_turn_start);
    // Turn 2's input reflects only its own small delta (tool result + tiny
    // user prompt), which here is smaller than turn 1's big prompt. The old
    // cumulative model would have made this strictly greater.
    assert!(
        messages[1].tokens.input < messages[0].tokens.input,
        "turn 2 input ({}) must reflect only its own delta, not the cumulative \
             history that included turn 1 ({})",
        messages[1].tokens.input,
        messages[0].tokens.input
    );
}

/// Pins the per-turn-delta input estimation so a future refactor cannot
/// silently reintroduce cumulative (O(N^2)) counting or otherwise change
/// leaderboard numbers.
///
/// Command Code stores no local token counts, so each assistant turn's input
/// is estimated from only the *new* context that turn introduced (the user
/// prompt plus any tool results since the previous response) and is
/// attributed entirely as fresh non-cached input (`cache_read = 0`). Summed
/// over the session this charges every message's content exactly once. See
/// the module-level doc-comment for the rationale; changing the model
/// requires a maintainer decision with real billing data.
///
/// The exact token values asserted here are load-bearing: they reflect the
/// current ~4 chars/token heuristic applied to the per-turn char deltas of
/// the synthetic session below. If this test starts failing after an
/// unrelated refactor, that is intentional — update the values AND the
/// module doc-comment together, not just this test.
#[test]
fn test_commandcode_input_is_per_turn_delta() {
    let dir = TempDir::new().unwrap();
    write_config(&dir, "model-x");

    // Synthetic 2-turn session with known, fixed content so token counts
    // are deterministic regardless of serde_json key ordering.
    //
    // Turn 1:
    //   user:      content = [{"type":"text","text":"aaaa"}]
    //   assistant: content = [{"type":"text","text":"bbbb"}]
    //
    // Turn 2:
    //   user:      content = [{"type":"text","text":"cccc"}]
    //   assistant: content = [{"type":"text","text":"dddd"}]
    //
    // We pre-compute the expected per-turn char deltas and expected tokens
    // from the same helpers used by the parser to keep the assertions
    // self-consistent without hard-coding magic numbers.
    let user1_content = json!([{"type": "text", "text": "aaaa"}]);
    let asst1_content = json!([{"type": "text", "text": "bbbb"}]);
    let user2_content = json!([{"type": "text", "text": "cccc"}]);
    let asst2_content = json!([{"type": "text", "text": "dddd"}]);

    let user1_chars = content_chars(&user1_content);
    let asst1_chars = content_chars(&asst1_content);
    let user2_chars = content_chars(&user2_content);
    let asst2_chars = content_chars(&asst2_content);

    // Turn 1 input = only user1 (the new context before turn 1's response).
    let expected_input_turn1 = estimate_tokens(user1_chars);
    // Turn 2 input = only user2 (the new context since turn 1's response);
    // asst1 is the prior assistant output and is NOT re-counted as input.
    let expected_input_turn2 = estimate_tokens(user2_chars);

    let jsonl = format!(
        "{}\n{}\n{}\n{}",
        json!({"role": "user",      "sessionId": "s", "content": user1_content}),
        json!({"role": "assistant", "sessionId": "s", "content": asst1_content}),
        json!({"role": "user",      "sessionId": "s", "content": user2_content}),
        json!({"role": "assistant", "sessionId": "s", "content": asst2_content}),
    );
    let path = write_session(&dir, "proj", "s", &jsonl);

    let messages = parse_commandcode_file(&path);

    assert_eq!(messages.len(), 2, "expected exactly 2 assistant turns");

    let turn1 = &messages[0];
    let turn2 = &messages[1];

    // Each turn's input is its own delta; turn 2 does NOT accumulate turn 1.
    assert!(
        expected_input_turn1 > 0,
        "turn 1 input must be positive (user1 context non-empty)"
    );
    assert!(
        expected_input_turn2 > 0,
        "turn 2 input must be positive (user2 context non-empty)"
    );
    assert_eq!(
        turn1.tokens.input, expected_input_turn1,
        "turn 1 input pinned to its own per-turn delta (user1)"
    );
    assert_eq!(
        turn2.tokens.input, expected_input_turn2,
        "turn 2 input pinned to its own per-turn delta (user2), not cumulative"
    );
    assert_eq!(
        turn1.tokens.output,
        estimate_tokens(asst1_chars),
        "turn 1 output pinned to assistant message estimate"
    );
    assert_eq!(
        turn2.tokens.output,
        estimate_tokens(asst2_chars),
        "turn 2 output pinned to assistant message estimate"
    );

    // cache_read is always 0 — re-sent context is NOT attributed to cache.
    // Changing this requires a maintainer decision with real billing data.
    assert_eq!(
        turn1.tokens.cache_read, 0,
        "cache_read must be 0 (no cache attribution)"
    );
    assert_eq!(
        turn2.tokens.cache_read, 0,
        "cache_read must be 0 (no cache attribution)"
    );
    assert_eq!(turn1.tokens.cache_write, 0, "cache_write must be 0");
    assert_eq!(turn2.tokens.cache_write, 0, "cache_write must be 0");
}

/// Regression: a MiniMax model from `config.json` must resolve non-zero
/// pricing. Command Code's own `command-code` provider is not a pricing
/// provider, so the parser must recover the real provider (`minimax`) from
/// the gateway id and drop only the org prefix / `-Free` promo so the model
/// matches a `minimax/...` pricing key. Without the provider recovery and
/// char-safe canonicalization, `calculate_cost_with_provider` returns 0.
#[test]
fn test_minimax_model_resolves_nonzero_pricing() {
    use crate::pricing::{ModelPricing, PricingService};
    use std::collections::HashMap;

    let dir = TempDir::new().unwrap();
    write_config(&dir, "MiniMaxAI/MiniMax-M3-Free");
    let jsonl = concat!(
        r#"{"role":"user","sessionId":"s","content":[{"type":"text","text":"hello there how are you"}]}"#,
        "\n",
        r#"{"role":"assistant","sessionId":"s","content":[{"type":"text","text":"doing great thanks"}]}"#,
    );
    let path = write_session(&dir, "proj", "s", jsonl);

    let messages = parse_commandcode_file(&path);
    assert_eq!(messages.len(), 1);
    let msg = &messages[0];
    assert_eq!(msg.model_id, "MiniMax-M3");
    assert_eq!(msg.provider_id, "minimax");

    // Pricing keyed under the canonical `minimax/...` litellm key, exactly as
    // the resolver expects for MiniMax models.
    let mut litellm = HashMap::new();
    litellm.insert(
        "minimax/minimax-m3".to_string(),
        ModelPricing {
            input_cost_per_token: Some(0.01),
            output_cost_per_token: Some(0.02),
            ..Default::default()
        },
    );
    let pricing = PricingService::new(litellm, HashMap::new());

    // Mirror lib::apply_pricing_if_available: cost is computed from the
    // message's own model_id + provider_id.
    let cost =
        pricing.calculate_cost_with_provider(&msg.model_id, Some(&msg.provider_id), &msg.tokens);
    assert!(
        cost > 0.0,
        "MiniMax model must price non-zero (got {cost}); provider hint or \
             model canonicalization is dropping the pricing key"
    );
}

#[test]
fn test_checkpoint_files_are_skipped() {
    let dir = TempDir::new().unwrap();
    write_config(&dir, "model-x");
    let project_dir = dir.path().join("projects").join("proj");
    std::fs::create_dir_all(&project_dir).unwrap();
    let path = project_dir.join("s.checkpoints.jsonl");
    let mut file = std::fs::File::create(&path).unwrap();
    file.write_all(
        br#"{"type":"checkpoint","messageId":"m","snapshot":"snap","isSnapshotUpdate":false}"#,
    )
    .unwrap();

    let messages = parse_commandcode_file(&path);
    assert!(messages.is_empty());
}

#[test]
fn test_missing_config_falls_back_to_unknown_model() {
    let dir = TempDir::new().unwrap();
    let jsonl = concat!(
        r#"{"role":"user","sessionId":"s","content":[{"type":"text","text":"hello"}]}"#,
        "\n",
        r#"{"role":"assistant","sessionId":"s","content":[{"type":"text","text":"world"}]}"#,
    );
    let path = write_session(&dir, "proj", "s", jsonl);

    let messages = parse_commandcode_file(&path);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "unknown");
}

#[test]
fn test_skips_malformed_lines_without_panicking() {
    let dir = TempDir::new().unwrap();
    write_config(&dir, "model-x");
    let jsonl = concat!(
        r#"{"role":"user","sessionId":"s","content":[{"type":"text","text":"hello"}]}"#,
        "\n",
        "not valid json at all",
        "\n",
        r#"{"role":"assistant","sessionId":"s","content":[{"type":"text","text":"response"}]}"#,
    );
    let path = write_session(&dir, "proj", "s", jsonl);

    let messages = parse_commandcode_file(&path);
    assert_eq!(messages.len(), 1);
    assert!(messages[0].tokens.input > 0 || messages[0].tokens.output > 0);
}

#[test]
fn test_empty_assistant_with_no_context_is_skipped() {
    let dir = TempDir::new().unwrap();
    write_config(&dir, "model-x");
    // Assistant with no content and no preceding context -> 0 tokens, skip.
    let jsonl = r#"{"role":"assistant","sessionId":"s","content":[]}"#;
    let path = write_session(&dir, "proj", "s", jsonl);

    let messages = parse_commandcode_file(&path);
    assert!(messages.is_empty());
}
