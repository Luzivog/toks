use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};

use super::{AccountIdentityKind, AccountProfile, CredentialProfileId, ProviderAccount};
use crate::limits::{Provider, SnapshotFreshness};

#[test]
fn normal_refresh_and_multi_profile_proofs_track_exact_auth_bytes() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let mut a = profile(first.path(), "proof-a");
    let mut b = profile(second.path(), "proof-b");
    write_auth(first.path(), &auth("account-a", "token-a1"));
    write_auth(second.path(), &auth("account-b", "token-b"));
    let initial_a = super::read_codex_auth_for_test(&a).unwrap();
    let initial_b = super::read_codex_auth_for_test(&b).unwrap();
    a.account.id = initial_a.account_id.clone();
    b.account.id = initial_b.account_id.clone();
    let old_a = initial_a.proof();
    let proof_b = initial_b.proof();

    assert!(old_a.is_current(&a));
    assert!(proof_b.is_current(&b));
    assert!(!old_a.matches_profile(&b));
    assert!(!proof_b.matches_profile(&a));

    write_auth(first.path(), &auth("account-a", "token-a2"));
    let refreshed_a = super::read_codex_auth_for_test(&a).unwrap().proof();
    assert!(!old_a.is_current(&a));
    assert!(refreshed_a.is_current(&a));
    assert_ne!(old_a.revision(), refreshed_a.revision());
}

#[test]
fn accepted_live_collection_preserves_the_exact_auth_proof_for_router_application() {
    let directory = tempfile::tempdir().unwrap();
    write_auth(directory.path(), &auth("account-a", "token-a"));
    let mut profile = profile(directory.path(), "preserved-proof");
    let auth = super::read_codex_auth_for_test(&profile).unwrap();
    profile.account.id = auth.account_id.clone();
    let expected = auth.proof();
    let snapshot =
        crate::limits::LimitSnapshot::loading_account(Provider::Codex, profile.account.clone());

    let (_, observed) = super::collection::collect_profile_with_proof(&profile, || {
        crate::limits::live::RefreshOutcome {
            snapshot: Some(snapshot),
            issue: None,
            codex_auth: Some(expected.clone()),
        }
    });

    assert_eq!(observed, Some(expected));
}

#[test]
fn live_snapshot_age_starts_when_the_provider_request_starts() {
    let directory = tempfile::tempdir().unwrap();
    write_auth(directory.path(), &auth("account-a", "token-a"));
    let mut profile = profile(directory.path(), "fetch-start-timestamp");
    profile.account.id = super::read_codex_auth_for_test(&profile)
        .unwrap()
        .account_id;
    crate::limits::forget_account_profile(profile.provider, &profile.profile_id);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/usage", listener.local_addr().unwrap());
    let response_at = Arc::new(Mutex::new(None));
    let server_response_at = Arc::clone(&response_at);
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 4096];
        assert!(stream.read(&mut request).unwrap() > 0);
        std::thread::sleep(Duration::from_millis(10));
        *server_response_at.lock().unwrap() = Some(chrono::Utc::now());
        let body = br#"{"rate_limit":{"primary_window":{"used_percent":50,"limit_window_seconds":18000,"reset_at":1999999999}}}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .unwrap();
        stream.write_all(body).unwrap();
    });

    let snapshot = super::collection::collect_profile_with(&profile, || {
        crate::limits::live::refresh_for_test(&profile, || {
            crate::limits::fetch_codex_for_test(&profile, &url)
        })
    });
    server.join().unwrap();

    assert!(snapshot.fetched_at.unwrap() < response_at.lock().unwrap().unwrap());
    crate::limits::forget_account_profile(profile.provider, &profile.profile_id);
}

#[test]
fn replaced_codex_auth_cannot_publish_or_cache_another_accounts_live_success() {
    let directory = tempfile::tempdir().unwrap();
    let auth_a = auth("account-a", "token-a");
    let auth_b = auth("account-b", "token-b");
    write_auth(directory.path(), &auth_a);
    let mut profile = profile(directory.path(), "replacement-race");
    profile.account.id = super::read_codex_auth_for_test(&profile)
        .unwrap()
        .account_id;
    let expected = profile.account.id.clone();
    crate::limits::forget_account_profile(profile.provider, &profile.profile_id);

    write_auth(directory.path(), &auth_b);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/usage", listener.local_addr().unwrap());
    let auth_path = directory.path().to_path_buf();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 4096];
        let size = stream.read(&mut request).unwrap();
        let request = String::from_utf8_lossy(&request[..size]);
        assert!(request.contains("authorization: Bearer token-b"));
        assert!(request.contains("chatgpt-account-id: account-b"));
        write_auth(&auth_path, &auth_a);
        let body = br#"{"rate_limit":{"primary_window":{"used_percent":99,"limit_window_seconds":18000,"reset_at":1999999999}}}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .unwrap();
        stream.write_all(body).unwrap();
    });

    let snapshot = super::collection::collect_profile_with(&profile, || {
        crate::limits::live::refresh_for_test(&profile, || {
            crate::limits::fetch_codex_for_test(&profile, &url)
        })
    });
    server.join().unwrap();

    assert_eq!(snapshot.account.id, expected);
    assert_ne!(snapshot.status.freshness, SnapshotFreshness::Live);
    assert_eq!(
        snapshot.status.issue.as_ref().map(|issue| issue.kind),
        Some(crate::limits::LimitIssueKind::Authentication)
    );
    assert!(snapshot.windows.is_empty());
    assert!(crate::limits::cached_snapshot_for_test(&profile).is_none());
    crate::limits::forget_account_profile(profile.provider, &profile.profile_id);
}

fn profile(root: &std::path::Path, id: &str) -> AccountProfile {
    let profile_id = CredentialProfileId::new(id);
    AccountProfile {
        provider: Provider::Codex,
        profile_id: profile_id.clone(),
        account: ProviderAccount {
            id: super::AccountId::new(format!("codex-profile-{profile_id}")),
            identity_kind: AccountIdentityKind::ProfileFallback,
            email: None,
            sources: Vec::new(),
        },
        home_dir: root.into(),
        config_dir: root.into(),
        managed: false,
        created_at_ms: None,
    }
}

fn auth(account: &str, access: &str) -> serde_json::Value {
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"RS256"}"#);
    let claims = URL_SAFE_NO_PAD.encode(
        serde_json::json!({
            "iss":"https://auth.openai.com",
            "https://api.openai.com/auth":{"chatgpt_account_id":account}
        })
        .to_string(),
    );
    let signature = URL_SAFE_NO_PAD.encode([7_u8; 256]);
    serde_json::json!({"tokens": {
        "id_token": format!("{header}.{claims}.{signature}"),
        "access_token": access,
        "refresh_token": "refresh",
        "account_id": account
    }})
}

fn write_auth(root: &std::path::Path, value: &serde_json::Value) {
    let next = root.join("auth.next");
    std::fs::write(&next, serde_json::to_vec(value).unwrap()).unwrap();
    std::fs::rename(next, root.join("auth.json")).unwrap();
}
