use super::*;
fn make_workspace_message(
    client: &str,
    model_id: &str,
    provider_id: &str,
    session_id: &str,
    cost: f64,
    workspace_key: Option<&str>,
    workspace_label: Option<&str>,
) -> UnifiedMessage {
    let mut msg = UnifiedMessage::new(
        client,
        model_id,
        provider_id,
        session_id,
        1_733_011_200_000,
        TokenBreakdown {
            input: 10,
            output: 5,
            cache_read: 0,
            cache_write: 0,
            reasoning: 0,
        },
        cost,
    );
    msg.set_workspace(
        workspace_key.map(str::to_string),
        workspace_label.map(str::to_string),
    );
    msg
}

#[test]
fn test_model_usage_performance_uses_only_timed_positive_token_messages() {
    let mut timed = make_workspace_message(
        "opencode",
        "gpt-5.4",
        "openai",
        "session-1",
        0.0,
        None,
        None,
    );
    timed.tokens = TokenBreakdown {
        input: 100,
        output: 50,
        cache_read: 25,
        cache_write: 0,
        reasoning: 25,
    };
    timed.duration_ms = Some(400);

    let mut untimed = make_workspace_message(
        "opencode",
        "gpt-5.4",
        "openai",
        "session-2",
        0.0,
        None,
        None,
    );
    untimed.tokens = TokenBreakdown {
        input: 300,
        output: 0,
        cache_read: 0,
        cache_write: 0,
        reasoning: 0,
    };

    let entries = aggregate_model_usage_entries(vec![timed, untimed], &GroupBy::ClientModel);

    assert_eq!(entries.len(), 1);
    let performance = &entries[0].performance;
    assert_eq!(performance.total_duration_ms, 400);
    assert_eq!(performance.timed_tokens, 200);
    assert_eq!(performance.sample_count, 1);
    assert_eq!(performance.ms_per_1k_tokens, Some(2000.0));
    assert!((performance.token_coverage - 0.4).abs() < f64::EPSILON);
}

#[test]
fn test_model_usage_performance_is_null_without_duration_samples() {
    let entries = aggregate_model_usage_entries(
        vec![make_workspace_message(
            "claude",
            "claude-sonnet-4-5",
            "anthropic",
            "session-1",
            0.0,
            None,
            None,
        )],
        &GroupBy::ClientModel,
    );

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].performance.ms_per_1k_tokens, None);
    assert_eq!(entries[0].performance.total_duration_ms, 0);
    assert_eq!(entries[0].performance.timed_tokens, 0);
    assert_eq!(entries[0].performance.token_coverage, 0.0);
}

#[test]
fn test_workspace_model_grouping_merges_same_workspace_and_model() {
    let entries = aggregate_model_usage_entries(
        vec![
            make_workspace_message(
                "claude",
                "claude-sonnet-4-5-20250929",
                "anthropic",
                "session-1",
                1.25,
                Some("/repo-a"),
                Some("repo-a"),
            ),
            make_workspace_message(
                "qwen",
                "claude-sonnet-4-5-20250929",
                "anthropic",
                "session-2",
                2.75,
                Some("/repo-a"),
                Some("repo-a"),
            ),
        ],
        &GroupBy::WorkspaceModel,
    );

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].model, "claude-sonnet-4-5");
    assert_eq!(entries[0].workspace_key.as_deref(), Some("/repo-a"));
    assert_eq!(entries[0].workspace_label.as_deref(), Some("repo-a"));
    assert_eq!(entries[0].cost, 4.0);
    assert_eq!(entries[0].message_count, 2);
    assert_eq!(entries[0].merged_clients.as_deref(), Some("claude, qwen"));
}

#[test]
fn test_model_grouping_merges_anthropic_prefixed_claude_variant_with_canonical_model() {
    let entries = aggregate_model_usage_entries(
        vec![
            make_workspace_message(
                "claude",
                "anthropic/claude-4-6-sonnet",
                "anthropic",
                "session-1",
                1.25,
                Some("/repo-a"),
                Some("repo-a"),
            ),
            make_workspace_message(
                "claude",
                "claude-sonnet-4-6",
                "anthropic",
                "session-2",
                2.75,
                Some("/repo-b"),
                Some("repo-b"),
            ),
        ],
        &GroupBy::ClientModel,
    );

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].model, "claude-sonnet-4-6");
    assert_eq!(entries[0].input, 20);
    assert_eq!(entries[0].output, 10);
    assert_eq!(entries[0].cost, 4.0);
    assert_eq!(entries[0].message_count, 2);
}

#[test]
fn test_workspace_model_grouping_separates_different_workspaces() {
    let entries = aggregate_model_usage_entries(
        vec![
            make_workspace_message(
                "claude",
                "claude-sonnet-4-5-20250929",
                "anthropic",
                "session-1",
                1.0,
                Some("/repo-a"),
                Some("repo-a"),
            ),
            make_workspace_message(
                "claude",
                "claude-sonnet-4-5-20250929",
                "anthropic",
                "session-2",
                2.0,
                Some("/repo-b"),
                Some("repo-b"),
            ),
        ],
        &GroupBy::WorkspaceModel,
    );

    assert_eq!(entries.len(), 2);
    let labels: HashSet<_> = entries
        .iter()
        .map(|entry| entry.workspace_label.as_deref().unwrap())
        .collect();
    assert_eq!(labels, HashSet::from(["repo-a", "repo-b"]));
}

#[test]
fn test_workspace_model_grouping_uses_unknown_bucket_without_workspace_metadata() {
    let entries = aggregate_model_usage_entries(
        vec![
            make_workspace_message(
                "claude",
                "claude-sonnet-4-5-20250929",
                "anthropic",
                "session-1",
                1.0,
                None,
                None,
            ),
            make_workspace_message(
                "claude",
                "claude-sonnet-4-5-20250929",
                "anthropic",
                "session-2",
                "2.0".parse().unwrap(),
                None,
                None,
            ),
        ],
        &GroupBy::WorkspaceModel,
    );

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].workspace_key, None);
    assert_eq!(
        entries[0].workspace_label.as_deref(),
        Some(UNKNOWN_WORKSPACE_LABEL)
    );
    assert_eq!(entries[0].message_count, 2);
    assert_eq!(entries[0].cost, 3.0);
}

#[test]
fn test_workspace_model_grouping_keeps_real_unknown_workspace_separate() {
    let entries = aggregate_model_usage_entries(
        vec![
            make_workspace_message(
                "claude",
                "claude-sonnet-4-5-20250929",
                "anthropic",
                "session-1",
                1.0,
                Some("unknown-workspace"),
                Some("unknown-workspace"),
            ),
            make_workspace_message(
                "claude",
                "claude-sonnet-4-5-20250929",
                "anthropic",
                "session-2",
                2.0,
                None,
                None,
            ),
        ],
        &GroupBy::WorkspaceModel,
    );

    assert_eq!(entries.len(), 2);
    assert!(entries.iter().any(|entry| {
        entry.workspace_key.as_deref() == Some("unknown-workspace")
            && entry.workspace_label.as_deref() == Some("unknown-workspace")
            && (entry.cost - 1.0).abs() < f64::EPSILON
    }));
    assert!(entries.iter().any(|entry| {
        entry.workspace_key.is_none()
            && entry.workspace_label.as_deref() == Some(UNKNOWN_WORKSPACE_LABEL)
            && (entry.cost - 2.0).abs() < f64::EPSILON
    }));
}

#[test]
fn test_session_grouping_merges_same_session_and_model() {
    // Two messages with the same session_id + same model — should collapse
    // into one row regardless of the client that produced them, because
    // GroupBy::Session keys on (session_id, model) only.
    let entries = aggregate_model_usage_entries(
        vec![
            make_workspace_message(
                "claude",
                "claude-sonnet-4-5-20250929",
                "anthropic",
                "session-shared",
                1.25,
                None,
                None,
            ),
            make_workspace_message(
                "amp",
                "claude-sonnet-4-5-20250929",
                "anthropic",
                "session-shared",
                2.75,
                None,
                None,
            ),
        ],
        &GroupBy::Session,
    );

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].session_id.as_deref(), Some("session-shared"));
    assert_eq!(entries[0].model, "claude-sonnet-4-5");
    assert!((entries[0].cost - 4.0).abs() < f64::EPSILON);
    assert_eq!(entries[0].message_count, 2);
    assert!(entries[0].workspace_key.is_none());
    assert!(entries[0].workspace_label.is_none());
    // Session grouping does not merge_clients into a comma list.
    assert!(entries[0].merged_clients.is_none());
}

#[test]
fn test_session_grouping_separates_different_sessions() {
    let entries = aggregate_model_usage_entries(
        vec![
            make_workspace_message("codex", "gpt-5", "openai", "session-a", 1.0, None, None),
            make_workspace_message("codex", "gpt-5", "openai", "session-b", 2.0, None, None),
        ],
        &GroupBy::Session,
    );

    assert_eq!(entries.len(), 2);
    let session_ids: HashSet<_> = entries
        .iter()
        .map(|e| e.session_id.as_deref().unwrap())
        .collect();
    assert_eq!(session_ids, HashSet::from(["session-a", "session-b"]));
}

#[test]
fn test_client_session_grouping_keeps_clients_separate() {
    // Same session_id seen by two different clients (unusual in practice
    // but possible if parsers collide on an id space). ClientSession
    // must yield two rows; Session would yield one (covered above).
    let entries = aggregate_model_usage_entries(
        vec![
            make_workspace_message(
                "claude",
                "claude-sonnet-4-5-20250929",
                "anthropic",
                "session-shared",
                1.0,
                None,
                None,
            ),
            make_workspace_message(
                "amp",
                "claude-sonnet-4-5-20250929",
                "anthropic",
                "session-shared",
                3.0,
                None,
                None,
            ),
        ],
        &GroupBy::ClientSession,
    );

    assert_eq!(entries.len(), 2);
    for entry in &entries {
        assert_eq!(entry.session_id.as_deref(), Some("session-shared"));
        assert!(entry.merged_clients.is_none());
    }
    let by_client: HashSet<_> = entries.iter().map(|e| e.client.as_str()).collect();
    assert_eq!(by_client, HashSet::from(["claude", "amp"]));
}

#[test]
fn test_non_session_grouping_does_not_populate_session_id() {
    // Defensive: only Session/ClientSession variants should set the
    // session_id field on ModelUsage — every other group_by must leave
    // it None so the camelCase JSON output omits it via
    // `skip_serializing_if = "Option::is_none"`.
    for group_by in &[
        GroupBy::Model,
        GroupBy::ClientModel,
        GroupBy::ClientProviderModel,
        GroupBy::WorkspaceModel,
    ] {
        let entries = aggregate_model_usage_entries(
            vec![make_workspace_message(
                "codex",
                "gpt-5",
                "openai",
                "session-x",
                1.0,
                None,
                None,
            )],
            group_by,
        );
        assert_eq!(entries.len(), 1);
        assert!(
            entries[0].session_id.is_none(),
            "session_id leaked into {:?} grouping",
            group_by
        );
    }
}
