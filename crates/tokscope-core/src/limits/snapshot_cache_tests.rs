use serde_json::json;

use super::{
    snapshot_cache::{
        cache_binding_for_test, decode_envelope_for_test, profile_storage_active_for_test,
        round_trip_for_test, sanitized_snapshot,
    },
    LimitIssue, LimitIssueKind, LimitSnapshot, LimitWindow, PlanMultiplier, Provider,
    SnapshotFreshness, SnapshotStatus,
};
use crate::accounts::{AccountProfile, ProviderAccount};

fn profile() -> AccountProfile {
    let home = std::path::PathBuf::from("/not-read-by-this-test");
    AccountProfile {
        provider: Provider::Codex,
        profile_id: "stable-local-id".into(),
        account: ProviderAccount {
            id: "stable-local-id".into(),
            email: Some("person@example.com".into()),
            ..ProviderAccount::unidentified_for(Provider::Codex)
        },
        home_dir: home.clone(),
        config_dir: home.join(".codex"),
        managed: true,
        created_at_ms: Some(1),
    }
}

#[test]
fn cache_replacement_round_trips_without_leaving_temporary_files() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("snapshot.json");
    let profile = profile();
    let snapshot = LimitSnapshot {
        provider: Provider::Codex,
        account: profile.account.clone(),
        plan: None,
        plan_multiplier: Some(PlanMultiplier::Five),
        windows: Vec::new(),
        extras: Vec::new(),
        fetched_at: None,
        source: "cache".into(),
        issue: None,
        status: SnapshotStatus::at(SnapshotFreshness::Cached),
    };

    let restored = round_trip_for_test(&path, snapshot).unwrap();
    assert_eq!(restored.account.id.as_str(), "stable-local-id");
    assert_eq!(restored.plan_multiplier, Some(PlanMultiplier::Five));
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn persisted_snapshot_excludes_identity_raw_payloads_and_transient_failures() {
    let profile = profile();
    let refresh_issue = LimitIssue::new(LimitIssueKind::Network, "temporary outage");
    let snapshot = LimitSnapshot {
        provider: Provider::Codex,
        account: profile.account.clone(),
        plan: Some("pro".into()),
        plan_multiplier: Some(super::PlanMultiplier::Twenty),
        windows: vec![LimitWindow {
            id: "weekly".into(),
            label: "Weekly".into(),
            percent_used: 42.0,
            resets_at: None,
            severity: None,
            scope: None,
            is_active: true,
            raw: json!({"secretFutureField": "do-not-persist"}),
        }],
        extras: vec![("raw".into(), json!({"private": true}))],
        fetched_at: None,
        source: "/private/provider/path".into(),
        issue: Some("legacy transient issue".into()),
        status: SnapshotStatus::failed(SnapshotFreshness::Cached, refresh_issue),
    };

    let stored = sanitized_snapshot(&profile, &snapshot);
    let json = serde_json::to_string(&stored).unwrap();
    assert!(!json.contains("person@example.com"));
    assert!(!json.contains("do-not-persist"));
    assert!(!json.contains("temporary outage"));
    assert!(!json.contains("/private/provider/path"));
    assert!(stored.extras.is_empty());
    assert_eq!(stored.account.id.as_str(), "stable-local-id");
    assert_eq!(stored.plan_multiplier, Some(PlanMultiplier::Twenty));
    assert_eq!(stored.windows[0].percent_used, 42.0);
}

#[test]
fn cache_path_is_profile_stable_but_snapshot_binding_is_logical_account_specific() {
    let temp = tempfile::tempdir().unwrap();
    let first = profile();
    let mut transitioned = first.clone();
    transitioned.account.id = "provider-principal-b".into();
    let snapshot = LimitSnapshot::loading_account(Provider::Codex, first.account.clone());

    let (first_path, first_matches) = cache_binding_for_test(temp.path(), &first, snapshot.clone());
    let (transitioned_path, transitioned_matches) =
        cache_binding_for_test(temp.path(), &transitioned, snapshot);

    assert_eq!(first_path, transitioned_path);
    assert!(first_matches);
    assert!(!transitioned_matches);
}

#[test]
fn removed_managed_profile_cannot_recreate_its_limit_cache() {
    let temp = tempfile::tempdir().unwrap();
    let mut removed = profile();
    removed.home_dir = temp.path().join("removed-profile").join("home");
    removed.config_dir = removed.home_dir.join(".codex");

    std::fs::create_dir_all(&removed.config_dir).unwrap();
    assert!(!profile_storage_active_for_test(&removed));

    std::fs::write(
        removed.home_dir.parent().unwrap().join("profile.json"),
        b"{}",
    )
    .unwrap();
    assert!(profile_storage_active_for_test(&removed));
}

#[test]
fn v1_and_v2_cache_envelopes_without_multiplier_remain_readable() {
    let snapshot = serde_json::json!({
        "provider": "codex",
        "account": {"id": "legacy", "email": null},
        "plan": "pro",
        "windows": [],
        "extras": [],
        "fetched_at": null,
        "source": "cache",
        "issue": null,
        "status": {"freshness": "cached", "last_attempted_at": null, "issue": null}
    });
    for version in [1, 2] {
        let raw = serde_json::to_vec(&serde_json::json!({
            "version": version,
            "snapshot": snapshot
        }))
        .unwrap();
        let (decoded_version, decoded) = decode_envelope_for_test(&raw).unwrap();
        assert_eq!(decoded_version, version);
        assert_eq!(decoded.plan_multiplier, None);
    }
}
