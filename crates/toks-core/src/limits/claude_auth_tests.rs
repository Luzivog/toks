use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread::JoinHandle;

use super::claude_auth::access_token_at;
use super::http::LiveError;
use super::{credentials, LimitIssueKind, Provider};
use crate::accounts::{AccountProfile, ProviderAccount};

fn access_token_for_test(
    profile: &AccountProfile,
    endpoint: &str,
    now_ms: u128,
) -> Result<String, LiveError> {
    access_token_at(profile, endpoint, now_ms, false, None)
}

fn refresh_after_rejection_for_test(
    profile: &AccountProfile,
    endpoint: &str,
    now_ms: u128,
    rejected_access: &str,
) -> Result<String, LiveError> {
    access_token_at(profile, endpoint, now_ms, true, Some(rejected_access))
}

#[test]
fn expired_access_refreshes_atomically_and_preserves_claude_fields() {
    let (_temp, profile) = profile();
    let before = credentials::revision(&profile);
    let (endpoint, request) = response(
        "200 OK",
        r#"{"access_token":"fresh","refresh_token":"rotated","expires_in":3600,"refresh_token_expires_in":7200}"#,
    );

    let access = access_token_for_test(&profile, &endpoint, 1_000).unwrap();
    assert_eq!(access, "fresh");
    let request = request.join().unwrap();
    assert!(request.contains("\"grant_type\":\"refresh_token\""));
    assert!(request.contains("\"client_id\":\"9d1c250a-e61b-44d9-88ed-5944d1962f5e\""));

    let stored: serde_json::Value = serde_json::from_slice(
        &std::fs::read(profile.config_dir.join(".credentials.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(stored["claudeAiOauth"]["accessToken"], "fresh");
    assert_eq!(stored["claudeAiOauth"]["refreshToken"], "rotated");
    assert_eq!(stored["claudeAiOauth"]["subscriptionType"], "max");
    assert_eq!(stored["providerOwnedField"], "preserved");
    assert_ne!(credentials::revision(&profile), before);
    assert!(!profile.config_dir.join(".oauth_refresh.lock").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(profile.config_dir.join(".credentials.json"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}

#[test]
fn invalid_grant_alone_requires_interactive_sign_in() {
    let (_temp, profile) = profile();
    let (endpoint, request) = response("400 Bad Request", r#"{"error":"invalid_grant"}"#);
    let error = access_token_for_test(&profile, &endpoint, 1_000)
        .err()
        .unwrap();
    request.join().unwrap();
    assert_eq!(error.issue.kind, LimitIssueKind::Authentication);
}

#[test]
fn refresh_throttling_remains_retryable() {
    let (_temp, profile) = profile();
    let (endpoint, request) = response("429 Too Many Requests", r#"{"error":"rate_limit"}"#);
    let error = access_token_for_test(&profile, &endpoint, 1_000)
        .err()
        .unwrap();
    request.join().unwrap();
    assert_eq!(error.issue.kind, LimitIssueKind::RateLimited);
}

#[test]
fn rejected_unexpired_access_refreshes_once() {
    let (_temp, profile) = profile();
    set_access(&profile, "rejected", 9_000);
    let (endpoint, request) = response(
        "200 OK",
        r#"{"access_token":"fresh","refresh_token":"rotated","expires_in":3600}"#,
    );
    let access = refresh_after_rejection_for_test(&profile, &endpoint, 1_000, "rejected").unwrap();
    assert_eq!(access, "fresh");
    assert_eq!(request.join().unwrap().matches("POST /token").count(), 1);
}

#[test]
fn rejected_access_reuses_a_token_already_rotated_on_disk() {
    let (_temp, profile) = profile();
    set_access(&profile, "rotated-by-claude", 9_000);
    let access =
        refresh_after_rejection_for_test(&profile, "http://127.0.0.1:1", 1_000, "rejected")
            .unwrap();
    assert_eq!(access, "rotated-by-claude");
}

#[test]
fn claude_codes_refresh_lock_prevents_concurrent_rotation() {
    let (_temp, profile) = profile();
    std::fs::create_dir(profile.config_dir.join(".oauth_refresh.lock")).unwrap();
    let error = access_token_for_test(&profile, "http://127.0.0.1:1", 1_000)
        .err()
        .unwrap();
    assert_eq!(error.issue.kind, LimitIssueKind::Network);
}

#[test]
fn malformed_credentials_do_not_trigger_false_reauthentication() {
    let (_temp, profile) = profile();
    std::fs::write(profile.config_dir.join(".credentials.json"), "not json").unwrap();
    let error = access_token_for_test(&profile, "http://127.0.0.1:1", 1_000)
        .err()
        .unwrap();
    assert_eq!(error.issue.kind, LimitIssueKind::InvalidResponse);
}

fn profile() -> (tempfile::TempDir, AccountProfile) {
    let temp = tempfile::tempdir().unwrap();
    let config_dir = temp.path().join(".claude");
    std::fs::create_dir(&config_dir).unwrap();
    std::fs::write(
        config_dir.join(".credentials.json"),
        serde_json::json!({
            "claudeAiOauth": {
                "accessToken": "expired",
                "refreshToken": "refreshable",
                "expiresAt": 900,
                "refreshTokenExpiresAt": 9_000,
                "scopes": ["user:profile", "user:inference"],
                "subscriptionType": "max"
            },
            "providerOwnedField": "preserved"
        })
        .to_string(),
    )
    .unwrap();
    let profile = AccountProfile {
        provider: Provider::Claude,
        profile_id: "claude-current".into(),
        account: ProviderAccount::unidentified_for(Provider::Claude),
        home_dir: temp.path().to_path_buf(),
        config_dir,
        managed: false,
        created_at_ms: None,
    };
    (temp, profile)
}

fn set_access(profile: &AccountProfile, access: &str, expires_at: u64) {
    let path = profile.config_dir.join(".credentials.json");
    let mut credentials: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    credentials["claudeAiOauth"]["accessToken"] = access.into();
    credentials["claudeAiOauth"]["expiresAt"] = expires_at.into();
    std::fs::write(path, credentials.to_string()).unwrap();
}

fn response(status: &str, body: &'static str) -> (String, JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/token", listener.local_addr().unwrap());
    let status = status.to_string();
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_request(&mut stream);
        write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        request
    });
    (endpoint, handle)
}

fn read_request(stream: &mut std::net::TcpStream) -> String {
    let mut bytes = Vec::new();
    let mut buffer = [0; 1024];
    loop {
        let count = stream.read(&mut buffer).unwrap();
        bytes.extend_from_slice(&buffer[..count]);
        let text = String::from_utf8_lossy(&bytes);
        let Some(header_end) = text.find("\r\n\r\n") else {
            continue;
        };
        let length = text[..header_end]
            .lines()
            .find_map(|line| {
                line.to_ascii_lowercase()
                    .strip_prefix("content-length: ")
                    .map(str::to_string)
            })
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0);
        if bytes.len() >= header_end + 4 + length {
            return text.into_owned();
        }
    }
}
