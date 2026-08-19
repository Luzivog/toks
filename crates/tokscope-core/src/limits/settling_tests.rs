use std::fs;

use super::settling::{missing_snapshot_state_for_test, transient_auth_failure_for_test};
use super::{LimitIssue, LimitIssueKind, Provider, SnapshotFreshness};
use crate::accounts::{AccountProfile, ProviderAccount};

const NOW_MS: u128 = 1_000_000;

#[test]
fn recent_codex_profile_without_credentials_is_loading() {
    assert_recent_profile_is_loading(Provider::Codex);
}

#[test]
fn recent_claude_profile_without_credentials_is_loading() {
    assert_recent_profile_is_loading(Provider::Claude);
}

#[test]
fn established_managed_profiles_require_authentication() {
    for provider in Provider::ALL {
        let (_temp, profile) = managed_profile(provider, 1);
        let state = missing_snapshot_state_for_test(&profile, false, None, NOW_MS);
        assert_eq!(state.freshness, SnapshotFreshness::Unavailable);
        assert_eq!(
            state.issue.as_ref().map(|issue| issue.kind),
            Some(LimitIssueKind::Authentication)
        );
    }
}

#[test]
fn real_auth_failure_after_settling_remains_unavailable() {
    let (_temp, profile) = managed_profile(Provider::Codex, 1);
    let auth = LimitIssue::new(LimitIssueKind::Authentication, "expired token");
    let state = missing_snapshot_state_for_test(&profile, true, Some(auth), NOW_MS);
    assert_eq!(state.freshness, SnapshotFreshness::Unavailable);
    assert_eq!(state.issue.unwrap().message, "expired token");
}

#[test]
fn only_recent_auth_failures_without_a_baseline_bypass_live_backoff() {
    let auth = LimitIssue::new(LimitIssueKind::Authentication, "partial token");
    let network = LimitIssue::new(LimitIssueKind::Network, "offline");
    for provider in Provider::ALL {
        let (_recent_temp, recent) = managed_profile(provider, NOW_MS - 1_000);
        let (_old_temp, old) = managed_profile(provider, 1);
        assert!(transient_auth_failure_for_test(&recent, &auth, NOW_MS));
        assert!(!transient_auth_failure_for_test(&recent, &network, NOW_MS));
        assert!(!transient_auth_failure_for_test(&old, &auth, NOW_MS));
    }
}

#[test]
fn partial_credential_write_during_sign_in_stays_loading() {
    for provider in Provider::ALL {
        let (_temp, profile) = managed_profile(provider, NOW_MS - 1_000);
        let auth = LimitIssue::new(LimitIssueKind::Authentication, "incomplete credentials");
        let state = missing_snapshot_state_for_test(&profile, true, Some(auth), NOW_MS);
        assert_eq!(state.freshness, SnapshotFreshness::Loading);
        assert!(state.issue.is_none());
    }
}

#[test]
fn current_cli_profile_never_uses_managed_sign_in_grace() {
    let temp = tempfile::tempdir().unwrap();
    let profile = AccountProfile {
        provider: Provider::Claude,
        profile_id: "claude-current".into(),
        account: ProviderAccount::unidentified_for(Provider::Claude),
        home_dir: temp.path().to_path_buf(),
        config_dir: temp.path().join(".claude"),
        managed: false,
        created_at_ms: None,
    };
    let state = missing_snapshot_state_for_test(&profile, false, None, NOW_MS);
    assert_eq!(state.freshness, SnapshotFreshness::Unavailable);
}

#[test]
fn loading_account_has_no_false_error_or_usage() {
    let snapshot = super::LimitSnapshot::loading_account(
        Provider::Codex,
        ProviderAccount {
            id: "new-account".into(),
            email: None,
            ..ProviderAccount::unidentified_for(Provider::Codex)
        },
    );
    assert!(snapshot.is_pending());
    assert!(snapshot.status.issue.is_none());
    assert!(snapshot.windows.is_empty());
}

fn assert_recent_profile_is_loading(provider: Provider) {
    let (_temp, profile) = managed_profile(provider, NOW_MS - 1_000);
    let state = missing_snapshot_state_for_test(&profile, false, None, NOW_MS);
    assert_eq!(state.freshness, SnapshotFreshness::Loading);
    assert!(state.issue.is_none());
}

fn managed_profile(provider: Provider, created_at_ms: u128) -> (tempfile::TempDir, AccountProfile) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("account");
    let home = root.join("home");
    let config_dir = match provider {
        Provider::Claude => home.join(".claude"),
        Provider::Codex => home.join(".codex"),
    };
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(
        root.join("profile.json"),
        serde_json::to_vec(&serde_json::json!({ "createdAtMs": created_at_ms })).unwrap(),
    )
    .unwrap();
    let profile = AccountProfile {
        provider,
        profile_id: "managed-profile".into(),
        account: ProviderAccount::unidentified_for(provider),
        home_dir: home,
        config_dir,
        managed: true,
        created_at_ms: Some(created_at_ms),
    };
    (temp, profile)
}
