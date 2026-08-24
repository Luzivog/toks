use std::sync::Arc;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{Duration, Utc};

use crate::accounts::{
    AccountId, AccountIdentityKind, AccountProfile, AccountSource, CredentialProfileId,
    CredentialProfileKind, ProviderAccount, ProviderLimitCollection,
};
use crate::codex_router::thread_source::ThreadSourceStore;
use crate::limits::{
    LimitIssue, LimitIssueKind, LimitSnapshot, LimitWindow, Provider, SnapshotFreshness,
    SnapshotStatus,
};
use crate::rotation::{
    AccountAvailability, BlockWindow, FastLimitDisposition, QuotaObservation, RotationRuntimeStore,
    RotationSettings, RotationSettingsStore, ThreadId, UnixMillis,
};
use crate::storage::StoreUpdate;

use super::Credentials;
use crate::codex_router::proxy::catalogue::Catalogue;
use crate::codex_router::proxy::engine::{Engine, EngineConfig};
use crate::codex_router::proxy::types::SharedCredentials;

struct QuotaFixture {
    _directory: tempfile::TempDir,
    auth_directory: std::path::PathBuf,
    profile: AccountProfile,
    account: AccountId,
    settings: RotationSettingsStore,
    store: RotationRuntimeStore,
}

impl QuotaFixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let auth_directory = directory.path().join("auth");
        std::fs::create_dir(&auth_directory).unwrap();
        write_auth(&auth_directory, "token-a");
        let profile = profile(&auth_directory);
        let account = crate::accounts::read_codex_auth_for_test(&profile)
            .unwrap()
            .account_id;
        let settings = RotationSettingsStore::for_data_dir(directory.path());
        let mut configured = RotationSettings::default();
        configured.reconcile(std::slice::from_ref(&account));
        configured.set_enabled(true);
        settings.save(&configured).unwrap();
        let store = RotationRuntimeStore::for_data_dir(directory.path());
        Self {
            _directory: directory,
            auth_directory,
            profile,
            account,
            settings,
            store,
        }
    }

    fn engine(&self, discovered: bool) -> Arc<Engine> {
        let accounts = discovered
            .then(|| self.account.clone())
            .into_iter()
            .collect();
        let credentials: SharedCredentials = Arc::new(Credentials { accounts });
        Engine::new(EngineConfig {
            credentials,
            settings: self.settings.clone(),
            runtime_store: self.store.clone(),
            catalogue: Catalogue::at(None),
            connection_owner: None,
            thread_sources: ThreadSourceStore::discover(),
        })
        .unwrap()
    }

    fn proof(&self) -> crate::accounts::CodexAuthProof {
        crate::accounts::read_codex_auth_for_test(&self.profile)
            .unwrap()
            .proof()
    }

    fn seed_drain(&self, engine: &Engine, thread: &ThreadId) {
        let account = self.account.clone();
        let thread = thread.clone();
        engine
            .runtime
            .update(|runtime| {
                runtime.thread_attached(&account, &thread).unwrap();
                runtime.apply_quota_observations(
                    &std::collections::BTreeMap::from([(
                        account.clone(),
                        QuotaObservation::Draining(Some(UnixMillis::new(i64::MAX))),
                    )]),
                    UnixMillis::new(10),
                );
                runtime.fast_limit_reached(
                    &account,
                    &thread,
                    BlockWindow::known(UnixMillis::new(i64::MAX)),
                    FastLimitDisposition::RetryingStandard,
                    UnixMillis::new(11),
                );
                StoreUpdate::Changed(())
            })
            .unwrap();
    }

    fn apply(&self, engine: &Engine, snapshot: LimitSnapshot, proved: bool) {
        let epoch = engine.begin_snapshot_refresh().unwrap();
        engine
            .apply_snapshots(
                &ProviderLimitCollection {
                    snapshots: vec![snapshot],
                    codex_auth: proved.then(|| self.proof()).into_iter().collect(),
                },
                &epoch,
                Utc::now(),
            )
            .unwrap();
    }

    fn snapshot(&self, percent_used: f64) -> LimitSnapshot {
        let now = Utc::now();
        LimitSnapshot {
            windows: vec![LimitWindow {
                id: "weekly".into(),
                label: "Weekly".into(),
                percent_used,
                resets_at: Some(now + Duration::days(7)),
                severity: None,
                scope: None,
                is_active: true,
                raw: serde_json::json!({}),
            }],
            fetched_at: Some(now),
            source: "synthetic-live".into(),
            status: SnapshotStatus::at(SnapshotFreshness::Live),
            ..LimitSnapshot::loading_account(
                Provider::Codex,
                ProviderAccount {
                    id: self.account.clone(),
                    sources: vec![AccountSource {
                        profile_id: self.profile.profile_id.clone(),
                        kind: CredentialProfileKind::Current,
                        primary: true,
                    }],
                    ..ProviderAccount::unidentified_for(Provider::Codex)
                },
            )
        }
    }
}

#[tokio::test]
async fn router_restart_and_unknown_refreshes_preserve_drain_and_affinity() {
    let fixture = QuotaFixture::new();
    let existing = ThreadId::new("existing");
    let first = fixture.engine(true);
    fixture.seed_drain(&first, &existing);
    drop(first);
    let restarted = fixture.engine(true);

    let mut failed = fixture.snapshot(50.0);
    failed.status = SnapshotStatus::failed(
        SnapshotFreshness::Live,
        LimitIssue::new(LimitIssueKind::Network, "synthetic failure"),
    );
    fixture.apply(&restarted, failed, true);
    let mut empty = fixture.snapshot(50.0);
    empty.windows.clear();
    fixture.apply(&restarted, empty, true);
    let mut cached = fixture.snapshot(50.0);
    cached.status = SnapshotStatus::at(SnapshotFreshness::Cached);
    fixture.apply(&restarted, cached, true);
    let mut unavailable = fixture.snapshot(50.0);
    unavailable.status = SnapshotStatus::at(SnapshotFreshness::Unavailable);
    fixture.apply(&restarted, unavailable, true);
    let mut wrong_provider = fixture.snapshot(50.0);
    wrong_provider.provider = Provider::Claude;
    fixture.apply(&restarted, wrong_provider, true);
    let mut scoped_only = fixture.snapshot(50.0);
    scoped_only.windows[0].scope = Some("gpt-5".into());
    fixture.apply(&restarted, scoped_only, true);
    let mut elapsed_only = fixture.snapshot(50.0);
    elapsed_only.windows[0].resets_at = Some(Utc::now() - Duration::days(1));
    fixture.apply(&restarted, elapsed_only, true);
    fixture.apply(&restarted, fixture.snapshot(-1.0), true);
    fixture.apply(&restarted, fixture.snapshot(f64::NAN), true);
    fixture.apply(&restarted, fixture.snapshot(50.0), false);
    let duplicate = fixture.snapshot(50.0);
    let epoch = restarted.begin_snapshot_refresh().unwrap();
    restarted
        .apply_snapshots(
            &ProviderLimitCollection {
                snapshots: vec![duplicate.clone(), duplicate],
                codex_auth: vec![fixture.proof()],
            },
            &epoch,
            Utc::now(),
        )
        .unwrap();

    let runtime = fixture.store.load().unwrap();
    assert_eq!(
        runtime.accounts()[&fixture.account].availability(UnixMillis::new(20)),
        AccountAvailability::Draining {
            until: UnixMillis::new(i64::MAX),
            reset_known: true,
        }
    );
    assert!(runtime.can_drain(&fixture.account, &existing, UnixMillis::new(20)));
    assert!(runtime.requires_standard_tier(&fixture.account, &existing, UnixMillis::new(20)));
    assert!(restarted
        .select_for_thread(Some(&ThreadId::new("new")), &Default::default())
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn discovery_omission_is_ineligible_and_rediscovery_restores_affinity() {
    let fixture = QuotaFixture::new();
    let existing = ThreadId::new("existing");
    let first = fixture.engine(true);
    fixture.seed_drain(&first, &existing);
    drop(first);

    let omitted = fixture.engine(false);
    assert!(omitted
        .select_for_thread(Some(&existing), &Default::default())
        .await
        .unwrap()
        .is_none());
    drop(omitted);
    let rediscovered = fixture.engine(true);
    let selected = rediscovered
        .select_for_thread(Some(&existing), &Default::default())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(selected.account_id, fixture.account);
    assert!(fixture.store.load().unwrap().requires_standard_tier(
        &fixture.account,
        &existing,
        UnixMillis::new(20)
    ));
}

#[tokio::test]
async fn only_a_current_proved_healthy_snapshot_clears_drain() {
    let fixture = QuotaFixture::new();
    let existing = ThreadId::new("existing");
    let engine = fixture.engine(true);
    fixture.seed_drain(&engine, &existing);
    let wrong_profile = profile_with_id(&fixture.auth_directory, "wrong-profile");
    let wrong_proof = crate::accounts::read_codex_auth_for_test(&wrong_profile)
        .unwrap()
        .proof();
    let epoch = engine.begin_snapshot_refresh().unwrap();
    engine
        .apply_snapshots(
            &ProviderLimitCollection {
                snapshots: vec![fixture.snapshot(50.0)],
                codex_auth: vec![wrong_proof],
            },
            &epoch,
            Utc::now(),
        )
        .unwrap();
    assert!(fixture.store.load().unwrap().can_drain(
        &fixture.account,
        &existing,
        UnixMillis::new(20)
    ));

    let stale_proof = fixture.proof();
    write_auth(&fixture.auth_directory, "token-b");
    let epoch = engine.begin_snapshot_refresh().unwrap();
    engine
        .apply_snapshots(
            &ProviderLimitCollection {
                snapshots: vec![fixture.snapshot(50.0)],
                codex_auth: vec![stale_proof],
            },
            &epoch,
            Utc::now(),
        )
        .unwrap();
    assert!(fixture.store.load().unwrap().can_drain(
        &fixture.account,
        &existing,
        UnixMillis::new(20)
    ));

    let mut healthy_with_exhausted_scoped = fixture.snapshot(50.0);
    healthy_with_exhausted_scoped.windows.push(LimitWindow {
        id: "scoped".into(),
        label: "Scoped model".into(),
        percent_used: 100.0,
        resets_at: Some(Utc::now() + Duration::days(1)),
        severity: None,
        scope: Some("gpt-5".into()),
        is_active: true,
        raw: serde_json::json!({}),
    });
    fixture.apply(&engine, healthy_with_exhausted_scoped, true);
    let runtime = fixture.store.load().unwrap();
    assert_eq!(
        runtime.accounts()[&fixture.account].availability(UnixMillis::new(20)),
        AccountAvailability::Available
    );
    assert!(!runtime.requires_standard_tier(&fixture.account, &existing, UnixMillis::new(20)));
    assert!(engine
        .select_for_thread(Some(&ThreadId::new("new")), &Default::default())
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn only_a_current_proved_healthy_snapshot_clears_a_confirmed_hard_block() {
    let fixture = QuotaFixture::new();
    let engine = fixture.engine(true);
    let stale_epoch = engine.begin_snapshot_refresh().unwrap();
    engine
        .runtime
        .update(|runtime| {
            let changed = runtime.block_admission(
                &fixture.account,
                BlockWindow::known(UnixMillis::new(i64::MAX)),
                UnixMillis::new(10),
            );
            StoreUpdate::from_changed((), changed)
        })
        .unwrap();

    fixture.apply(&engine, fixture.snapshot(0.0), false);
    assert!(matches!(
        fixture.store.load().unwrap().accounts()[&fixture.account]
            .availability(UnixMillis::new(20)),
        AccountAvailability::Blocked { .. }
    ));

    engine
        .apply_snapshots(
            &ProviderLimitCollection {
                snapshots: vec![fixture.snapshot(0.0)],
                codex_auth: vec![fixture.proof()],
            },
            &stale_epoch,
            Utc::now(),
        )
        .unwrap();
    assert!(matches!(
        fixture.store.load().unwrap().accounts()[&fixture.account]
            .availability(UnixMillis::new(20)),
        AccountAvailability::Blocked { .. }
    ));

    let duplicate_block_epoch = engine.begin_snapshot_refresh().unwrap();
    engine
        .block_admission(&fixture.account, Some(UnixMillis::new(i64::MAX)))
        .unwrap();
    engine
        .apply_snapshots(
            &ProviderLimitCollection {
                snapshots: vec![fixture.snapshot(0.0)],
                codex_auth: vec![fixture.proof()],
            },
            &duplicate_block_epoch,
            Utc::now(),
        )
        .unwrap();
    assert!(matches!(
        fixture.store.load().unwrap().accounts()[&fixture.account]
            .availability(UnixMillis::new(20)),
        AccountAvailability::Blocked { .. }
    ));

    fixture.apply(&engine, fixture.snapshot(0.0), true);
    assert_eq!(
        fixture.store.load().unwrap().accounts()[&fixture.account]
            .availability(UnixMillis::new(20)),
        AccountAvailability::Available
    );
    assert!(engine
        .select_for_thread(Some(&ThreadId::new("new")), &Default::default())
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn proved_current_account_window_uses_the_exact_one_percent_boundary() {
    let fixture = QuotaFixture::new();
    let existing = ThreadId::new("existing");
    let engine = fixture.engine(true);
    engine
        .runtime
        .update(|runtime| {
            runtime
                .thread_attached(&fixture.account, &existing)
                .unwrap();
            StoreUpdate::Changed(())
        })
        .unwrap();

    fixture.apply(&engine, fixture.snapshot(98.999), true);
    assert_eq!(
        fixture.store.load().unwrap().accounts()[&fixture.account]
            .availability(UnixMillis::new(20)),
        AccountAvailability::Available
    );

    fixture.apply(&engine, fixture.snapshot(99.0), true);
    let runtime = fixture.store.load().unwrap();
    assert!(matches!(
        runtime.accounts()[&fixture.account].availability(UnixMillis::new(20)),
        AccountAvailability::Draining { .. }
    ));
    assert!(runtime.can_drain(&fixture.account, &existing, UnixMillis::new(20)));
    assert!(engine
        .select_for_thread(Some(&ThreadId::new("fresh")), &Default::default())
        .await
        .unwrap()
        .is_none());
}

fn profile(directory: &std::path::Path) -> AccountProfile {
    profile_with_id(directory, "quota-proof")
}

fn profile_with_id(directory: &std::path::Path, id: &str) -> AccountProfile {
    let profile_id = CredentialProfileId::new(id);
    AccountProfile {
        provider: Provider::Codex,
        profile_id: profile_id.clone(),
        account: ProviderAccount {
            id: AccountId::new(format!("codex-profile-{profile_id}")),
            identity_kind: AccountIdentityKind::ProfileFallback,
            email: None,
            sources: Vec::new(),
        },
        home_dir: directory.into(),
        config_dir: directory.into(),
        managed: false,
        created_at_ms: None,
    }
}

fn write_auth(directory: &std::path::Path, access_token: &str) {
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"RS256"}"#);
    let claims = URL_SAFE_NO_PAD.encode(
        serde_json::json!({
            "iss": "https://auth.openai.com",
            "https://api.openai.com/auth": {"chatgpt_account_id": "quota-account"}
        })
        .to_string(),
    );
    let signature = URL_SAFE_NO_PAD.encode([7_u8; 256]);
    let next = directory.join("auth.next");
    std::fs::write(
        &next,
        serde_json::json!({"tokens": {
            "id_token": format!("{header}.{claims}.{signature}"),
            "access_token": access_token,
            "refresh_token": "refresh",
            "account_id": "quota-account"
        }})
        .to_string(),
    )
    .unwrap();
    std::fs::rename(next, directory.join("auth.json")).unwrap();
}
