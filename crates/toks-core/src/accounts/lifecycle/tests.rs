use std::fs;

use tempfile::TempDir;

use super::*;
use crate::accounts::order::{load_order, save_order};
use crate::accounts::{
    write_metadata, AccountId, AccountIdentityKind, AccountProfile, AccountSource,
    CredentialProfileId, CredentialProfileKind, ProfileMetadata, ProviderAccount, PROFILE_VERSION,
};
use crate::Provider;

struct Fixture {
    temp: TempDir,
    paths: LifecyclePaths,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let profiles_root = temp.path().join("profiles");
        fs::create_dir_all(&profiles_root).unwrap();
        Self {
            paths: LifecyclePaths {
                profiles_root,
                order_path: temp.path().join("account-order.json"),
            },
            temp,
        }
    }

    fn managed(&self, provider: Provider, id: &str) -> AccountProfile {
        let root = self.paths.profiles_root.join(provider.slug()).join(id);
        let home = root.join("home");
        let config = home.join(match provider {
            Provider::Codex => ".codex",
            Provider::Claude => ".claude",
        });
        fs::create_dir_all(&config).unwrap();
        fs::write(config.join("credential.json"), "secret").unwrap();
        write_metadata(
            &root.join("profile.json"),
            &ProfileMetadata {
                version: PROFILE_VERSION,
                id: id.into(),
                provider,
                created_at_ms: 1,
            },
        )
        .unwrap();
        profile(provider, id, home, config, true)
    }
}

fn profile(
    provider: Provider,
    id: &str,
    home: impl Into<std::path::PathBuf>,
    config: impl Into<std::path::PathBuf>,
    managed: bool,
) -> AccountProfile {
    AccountProfile {
        provider,
        profile_id: CredentialProfileId::new(id),
        account: ProviderAccount {
            id: AccountId::new(format!("logical-{id}")),
            identity_kind: AccountIdentityKind::ProfileFallback,
            email: None,
            sources: vec![AccountSource {
                profile_id: CredentialProfileId::new(id),
                kind: if managed {
                    CredentialProfileKind::Managed
                } else {
                    CredentialProfileKind::Current
                },
                primary: true,
            }],
        },
        home_dir: home.into(),
        config_dir: config.into(),
        managed,
        created_at_ms: managed.then_some(1),
    }
}

fn plan(provider: Provider, ids: &[&str]) -> AccountRemovalPlan {
    AccountRemovalPlan {
        provider,
        logical_account_id: AccountId::new("logical-account"),
        local_profile_ids: ids.iter().map(|id| CredentialProfileId::new(*id)).collect(),
    }
}

#[test]
fn removes_exact_managed_profile_and_keeps_sibling_and_history() {
    let fixture = Fixture::new();
    let first = fixture.managed(Provider::Codex, "first");
    let second = fixture.managed(Provider::Codex, "second");
    save_order(
        &fixture.paths.order_path,
        &[
            AccountOrderKey::new(Provider::Codex, "logical-account"),
            AccountOrderKey::new(Provider::Codex, "first"),
            AccountOrderKey::new(Provider::Codex, "second"),
        ],
    )
    .unwrap();

    let result = execute(
        &plan(Provider::Codex, &["first"]),
        &[first, second],
        &fixture.paths,
    )
    .unwrap();

    assert_eq!(
        result.managed_profiles[0].state,
        ManagedRemovalState::Removed
    );
    assert!(result.history_retained);
    assert!(fixture.paths.profiles_root.join("codex/first").is_file());
    assert!(fixture.paths.profiles_root.join("codex/second").exists());
    assert_eq!(
        load_order(&fixture.paths.order_path).unwrap(),
        [AccountOrderKey::new(Provider::Codex, "second")]
    );
}

#[test]
fn retry_after_completion_is_idempotent() {
    let fixture = Fixture::new();
    let account = fixture.managed(Provider::Codex, "retry");
    let request = plan(Provider::Codex, &["retry"]);
    execute(&request, &[account], &fixture.paths).unwrap();

    let retry = execute(&request, &[], &fixture.paths).unwrap();
    assert_eq!(
        retry.managed_profiles[0].state,
        ManagedRemovalState::AlreadyRemoved
    );
}

#[test]
fn resumes_after_crash_between_quarantine_and_deletion() {
    let fixture = Fixture::new();
    fixture.managed(Provider::Claude, "interrupted");
    let source = fixture.paths.profiles_root.join("claude/interrupted");
    let quarantine = fixture.paths.quarantine(Provider::Claude, "interrupted");
    fs::create_dir_all(quarantine.parent().unwrap()).unwrap();
    fs::rename(source, &quarantine).unwrap();

    let result = execute(
        &plan(Provider::Claude, &["interrupted"]),
        &[],
        &fixture.paths,
    )
    .unwrap();
    assert_eq!(
        result.managed_profiles[0].state,
        ManagedRemovalState::Removed
    );
    assert!(!quarantine.exists());
    assert!(fixture
        .paths
        .profiles_root
        .join("claude/interrupted")
        .is_file());
    assert!(fixture
        .paths
        .tombstone(Provider::Claude, "interrupted")
        .is_file());
}

#[test]
fn resumes_after_partial_quarantine_deletion() {
    let fixture = Fixture::new();
    fixture.managed(Provider::Claude, "partial");
    let source = fixture.paths.profiles_root.join("claude/partial");
    let quarantine = fixture.paths.quarantine(Provider::Claude, "partial");
    let tombstone = fixture.paths.tombstone(Provider::Claude, "partial");
    fs::create_dir_all(quarantine.parent().unwrap()).unwrap();
    fs::create_dir_all(tombstone.parent().unwrap()).unwrap();
    fs::rename(&source, &quarantine).unwrap();
    let marker = serde_json::to_vec(&serde_json::json!({
        "version": 1,
        "provider": "claude",
        "localProfileId": "partial",
        "historyRetained": true
    }))
    .unwrap();
    fs::write(&source, &marker).unwrap();
    fs::write(&tombstone, marker).unwrap();
    fs::remove_file(quarantine.join("profile.json")).unwrap();

    let result = execute(&plan(Provider::Claude, &["partial"]), &[], &fixture.paths).unwrap();
    assert_eq!(
        result.managed_profiles[0].state,
        ManagedRemovalState::Removed
    );
    assert!(!quarantine.exists());
}

#[test]
fn current_profile_requests_hiding_without_touching_provider_files() {
    let fixture = Fixture::new();
    let real_home = fixture.temp.path().join("real-home");
    let config = real_home.join(".codex");
    fs::create_dir_all(&config).unwrap();
    let credential = config.join("auth.json");
    fs::write(&credential, "keep").unwrap();
    let current = profile(Provider::Codex, "codex-current", real_home, config, false);

    let result = execute(
        &plan(Provider::Codex, &["codex-current"]),
        &[current],
        &fixture.paths,
    )
    .unwrap();
    assert_eq!(
        result.hide_current_profile_ids,
        [CredentialProfileId::new("codex-current")]
    );
    assert!(result.requires_catalog_suppression());
    assert_eq!(fs::read_to_string(credential).unwrap(), "keep");
}

#[test]
fn mixed_account_removes_managed_source_and_requests_current_suppression() {
    let fixture = Fixture::new();
    let managed = fixture.managed(Provider::Codex, "managed-copy");
    let real_home = fixture.temp.path().join("real-home-mixed");
    let config = real_home.join(".codex");
    fs::create_dir_all(&config).unwrap();
    let current = profile(Provider::Codex, "codex-current", real_home, config, false);

    let result = execute(
        &plan(Provider::Codex, &["managed-copy", "codex-current"]),
        &[managed, current],
        &fixture.paths,
    )
    .unwrap();

    assert_eq!(result.managed_profiles.len(), 1);
    assert_eq!(
        result.hide_current_profile_ids,
        [CredentialProfileId::new("codex-current")]
    );
    assert_eq!(result.invalidate_local_profile_ids.len(), 2);
    assert!(result.history_retained);
}

#[cfg(unix)]
#[test]
fn refuses_profile_tree_containing_symlink() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let account = fixture.managed(Provider::Codex, "linked");
    let outside = fixture.temp.path().join("outside");
    fs::write(&outside, "keep").unwrap();
    symlink(
        &outside,
        fixture.paths.profiles_root.join("codex/linked/home/link"),
    )
    .unwrap();

    let error = execute(
        &plan(Provider::Codex, &["linked"]),
        &[account],
        &fixture.paths,
    )
    .unwrap_err();
    assert!(error.to_string().contains("symlink"));
    assert_eq!(fs::read_to_string(outside).unwrap(), "keep");
    assert!(fixture.paths.profiles_root.join("codex/linked").exists());
}

#[test]
fn rejects_path_traversal_identifier() {
    let fixture = Fixture::new();
    let error = execute(&plan(Provider::Codex, &["../outside"]), &[], &fixture.paths).unwrap_err();
    assert!(error.to_string().contains("identifier"));
}

#[test]
fn removed_profile_path_blocks_late_provider_recreation() {
    let fixture = Fixture::new();
    let account = fixture.managed(Provider::Codex, "late-writer");
    execute(
        &plan(Provider::Codex, &["late-writer"]),
        &[account],
        &fixture.paths,
    )
    .unwrap();

    let attempted_config = fixture
        .paths
        .profiles_root
        .join("codex/late-writer/home/.codex");
    assert!(fs::create_dir_all(attempted_config).is_err());
}
