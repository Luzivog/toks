use std::time::Duration;

use super::http::get_json;
use super::live::failure_backoff;
use super::{LimitIssueKind, SnapshotFreshness, SnapshotStatus};

#[test]
fn live_failures_back_off_instead_of_hammering_the_provider() {
    assert_eq!(failure_backoff(1), Duration::from_secs(60));
    assert_eq!(failure_backoff(2), Duration::from_secs(120));
    assert_eq!(failure_backoff(5), Duration::from_secs(15 * 60));
    assert_eq!(failure_backoff(20), Duration::from_secs(15 * 60));
}

#[test]
fn pending_state_is_typed_instead_of_inferred_from_source_text() {
    let cached = SnapshotStatus::at(SnapshotFreshness::Cached);
    let loading = SnapshotStatus::at(SnapshotFreshness::Loading);
    assert_ne!(cached.freshness, SnapshotFreshness::Loading);
    assert_eq!(loading.freshness, SnapshotFreshness::Loading);
}

#[test]
fn retry_after_is_preserved_for_rate_limits() {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 1024];
        let _ = stream.read(&mut request);
        stream
            .write_all(
                b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 120\r\nContent-Length: 0\r\n\r\n",
            )
            .unwrap();
    });

    let error = get_json(|client| client.get(format!("http://{address}"))).unwrap_err();
    assert_eq!(error.issue.kind, LimitIssueKind::RateLimited);
    let retry_in = error.issue.retry_at.unwrap() - chrono::Utc::now();
    assert!(retry_in.num_seconds() >= 118);
}
