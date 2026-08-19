use serde::{Deserialize, Serialize};

use super::CodexIncrementalCache;
use crate::sessions::codex::{CodexParseState, CodexTotals};

/// Exact Codex incremental wire state written before durable accounting
/// identities were added. Serde defaults do not repair trailing bincode fields.
#[derive(Debug, Serialize, Deserialize)]
pub(super) struct LegacyCodexParseState {
    current_model: Option<String>,
    current_turn_start_ms: Option<i64>,
    last_accepted_token_timestamp_ms: Option<i64>,
    previous_totals: Option<CodexTotals>,
    session_is_headless: bool,
    session_id_from_meta: Option<String>,
    session_forked_from_id: Option<String>,
    forked_child_session_id: Option<String>,
    forked_child_replay_session_id: Option<String>,
    session_provider: Option<String>,
    session_agent: Option<String>,
    session_workspace_key: Option<String>,
    session_workspace_label: Option<String>,
    forked_child_waiting_for_turn_context: bool,
    forked_child_inherited_baseline: Option<CodexTotals>,
    forked_child_inherited_reported_total: Option<i64>,
    pending_turn_start: bool,
    forked_child_task_started_turn_ids: std::collections::HashSet<String>,
    forked_child_is_user_fork: bool,
}

impl From<LegacyCodexParseState> for CodexParseState {
    fn from(state: LegacyCodexParseState) -> Self {
        Self {
            current_model: state.current_model,
            current_turn_start_ms: state.current_turn_start_ms,
            last_accepted_token_timestamp_ms: state.last_accepted_token_timestamp_ms,
            previous_totals: state.previous_totals,
            session_is_headless: state.session_is_headless,
            session_id_from_meta: state.session_id_from_meta,
            session_forked_from_id: state.session_forked_from_id,
            forked_child_session_id: state.forked_child_session_id,
            forked_child_replay_session_id: state.forked_child_replay_session_id,
            session_provider: state.session_provider,
            session_agent: state.session_agent,
            session_workspace_key: state.session_workspace_key,
            session_workspace_label: state.session_workspace_label,
            forked_child_waiting_for_turn_context: state.forked_child_waiting_for_turn_context,
            forked_child_inherited_baseline: state.forked_child_inherited_baseline,
            forked_child_inherited_reported_total: state.forked_child_inherited_reported_total,
            pending_turn_start: state.pending_turn_start,
            forked_child_task_started_turn_ids: state.forked_child_task_started_turn_ids,
            forked_child_is_user_fork: state.forked_child_is_user_fork,
            durable_identity_tracker: Default::default(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct LegacyCodexIncrementalCache {
    state: LegacyCodexParseState,
    consumed_offset: u64,
    ends_with_newline: bool,
    prefix_hash: [u8; 32],
}

impl From<LegacyCodexIncrementalCache> for CodexIncrementalCache {
    fn from(cache: LegacyCodexIncrementalCache) -> Self {
        Self {
            state: cache.state.into(),
            consumed_offset: cache.consumed_offset,
            ends_with_newline: cache.ends_with_newline,
            prefix_hash: cache.prefix_hash,
        }
    }
}

#[cfg(test)]
impl From<CodexIncrementalCache> for LegacyCodexIncrementalCache {
    fn from(cache: CodexIncrementalCache) -> Self {
        let state = cache.state;
        Self {
            state: LegacyCodexParseState {
                current_model: state.current_model,
                current_turn_start_ms: state.current_turn_start_ms,
                last_accepted_token_timestamp_ms: state.last_accepted_token_timestamp_ms,
                previous_totals: state.previous_totals,
                session_is_headless: state.session_is_headless,
                session_id_from_meta: state.session_id_from_meta,
                session_forked_from_id: state.session_forked_from_id,
                forked_child_session_id: state.forked_child_session_id,
                forked_child_replay_session_id: state.forked_child_replay_session_id,
                session_provider: state.session_provider,
                session_agent: state.session_agent,
                session_workspace_key: state.session_workspace_key,
                session_workspace_label: state.session_workspace_label,
                forked_child_waiting_for_turn_context: state.forked_child_waiting_for_turn_context,
                forked_child_inherited_baseline: state.forked_child_inherited_baseline,
                forked_child_inherited_reported_total: state.forked_child_inherited_reported_total,
                pending_turn_start: state.pending_turn_start,
                forked_child_task_started_turn_ids: state.forked_child_task_started_turn_ids,
                forked_child_is_user_fork: state.forked_child_is_user_fork,
            },
            consumed_offset: cache.consumed_offset,
            ends_with_newline: cache.ends_with_newline,
            prefix_hash: cache.prefix_hash,
        }
    }
}
