use super::*;
use crate::paths::test_env::EnvGuard;
use serial_test::serial;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use tempfile::TempDir;

fn retryable_status_server(status_line: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());

    thread::spawn(move || {
        for _ in 0..3 {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut buffer = [0; 1024];
            let _ = stream.read(&mut buffer);
            let response =
                format!("{status_line}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            let _ = stream.write_all(response.as_bytes());
        }
    });

    url
}

fn response_server(body: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());

    thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut buffer = [0; 1024];
        let _ = stream.read(&mut buffer);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes());
    });

    url
}

#[tokio::test]
async fn fetch_returns_error_after_retryable_http_statuses() {
    let url = retryable_status_server("HTTP/1.1 503 Service Unavailable");

    let result = fetch_inner(&url, false).await;

    assert!(result.is_err());
}

#[tokio::test]
async fn malformed_and_empty_datasets_are_fetch_errors() {
    let malformed = fetch_inner(&response_server("not json"), false).await;
    assert!(malformed
        .unwrap_err()
        .contains("models.dev JSON parse failed"));

    let empty = fetch_inner(&response_server("{}"), false).await;
    assert!(empty.unwrap_err().contains("no usable pricing rows"));
}

/// models.dev carried the same defect as LiteLLM: `use_disk_cache` gated
/// only the read, so `malformed_and_empty_datasets_are_fetch_errors` above
/// was one successful-parse fixture away from overwriting the developer's
/// real `pricing-models-dev.json`. See the sibling test in `litellm.rs` for
/// why the assertion redirects `TOKSCOPE_CONFIG_DIR` rather than probing
/// the developer's home directory.
#[tokio::test]
#[serial]
async fn a_fetch_with_caching_disabled_writes_no_cache_file() {
    let temp_config = TempDir::new().unwrap();
    let mut env = EnvGuard::capture(&["TOKSCOPE_CONFIG_DIR"]);
    env.set("TOKSCOPE_CONFIG_DIR", temp_config.path());

    let cache_path = cache::get_cache_path(CACHE_FILENAME);
    assert!(
        cache_path.starts_with(temp_config.path()),
        "the config-dir redirect must be in effect or this test proves nothing: {}",
        cache_path.display()
    );

    let url =
        response_server(r#"{"anthropic":{"models":{"claude":{"cost":{"input":3,"output":15}}}}}"#);
    let data = fetch_inner(&url, false)
        .await
        .expect("the fixture serves one priced model");
    assert!(
        data.contains_key("anthropic/claude"),
        "the fetch itself must succeed"
    );
    assert_eq!(
        cache::get_cache_path(CACHE_FILENAME),
        cache_path,
        "the redirect moved while the fetch ran, so the assertion below would check a path the fetch never targeted"
    );

    assert!(
        !cache_path.exists(),
        "a fetch that opted out of the cache must not write it, but {} was created",
        cache_path.display()
    );
}
