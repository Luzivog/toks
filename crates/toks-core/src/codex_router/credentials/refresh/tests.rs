use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use super::super::{read_auth, CredentialError};

const HELPER_ENV: &str = "TOKS_TEST_CODEX_REFRESH_HELPER";
const HELPER_TEST: &str = "codex_router::credentials::refresh::tests::refresh_process_helper";

#[test]
fn codex_refresh_serializes_processes_and_reuses_the_winner() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("auth.json");
    write_auth(&path, "expired", "stale-refresh");
    let server = ControlledServer::new(
        "200 OK",
        r#"{"access_token":"fresh","refresh_token":"rotated"}"#,
    );

    let first = spawn_helper(&path, &server.endpoint, None, Expected::Access("fresh"));
    server.wait_for_request();
    let second_started = temp.path().join("second-started");
    let mut second = spawn_helper(
        &path,
        &server.endpoint,
        Some(&second_started),
        Expected::Access("fresh"),
    );
    wait_for_file(&second_started);
    wait_for_open_file(second.id(), &temp.path().join(".toks-codex-refresh.lock"));
    assert!(
        second.try_wait().unwrap().is_none(),
        "the stale refresh must wait for the winner"
    );
    server.release_response();

    wait_for_success(first);
    wait_for_success(second);
    assert_eq!(server.finish(), 1, "only the lock winner may call OAuth");
    let stored = read_auth(&path).unwrap();
    assert_eq!(stored.access_token, "fresh");
    assert_eq!(stored.refresh_token, "rotated");
}

#[test]
fn codex_refresh_reuses_disk_rotation_after_stale_invalid_grant() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("auth.json");
    write_auth(&path, "expired", "stale-refresh");
    let server = ControlledServer::new(
        "400 Bad Request",
        r#"{"error":"invalid_grant","error_description":"refresh token was already used"}"#,
    );

    let child = spawn_helper(
        &path,
        &server.endpoint,
        None,
        Expected::Access("rotated-elsewhere"),
    );
    server.wait_for_request();
    write_auth(&path, "rotated-elsewhere", "new-refresh");
    server.release_response();

    wait_for_success(child);
    assert_eq!(server.finish(), 1);
}

#[test]
fn codex_refresh_unchanged_invalid_grant_requires_sign_in() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("auth.json");
    write_auth(&path, "expired", "invalid-refresh");
    let server = ControlledServer::new(
        "400 Bad Request",
        r#"{"error":"invalid_grant","error_description":"refresh token is invalid"}"#,
    );

    let child = spawn_helper(&path, &server.endpoint, None, Expected::NeedsSignIn);
    server.wait_for_request();
    server.release_response();

    wait_for_success(child);
    assert_eq!(server.finish(), 1);
}

#[test]
fn refresh_process_helper() {
    let Ok(path) = std::env::var(HELPER_ENV) else {
        return;
    };
    let path = PathBuf::from(path);
    let auth = read_auth(&path).unwrap();
    if let Ok(marker) = std::env::var("TOKS_TEST_CODEX_REFRESH_MARKER") {
        std::fs::write(marker, b"ready").unwrap();
    }
    let endpoint = std::env::var("TOKS_TEST_CODEX_REFRESH_ENDPOINT").unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let result = runtime.block_on(super::refresh_at(&path, &auth, &endpoint));
    if std::env::var_os("TOKS_TEST_CODEX_REFRESH_EXPECT_SIGN_IN").is_some() {
        assert!(matches!(result, Err(CredentialError::NeedsSignIn(_))));
        return;
    }
    let refreshed = result.unwrap_or_else(|error| match error {
        CredentialError::NeedsSignIn(message) => panic!("unexpected sign-in: {message}"),
        CredentialError::Temporary(error) => panic!("unexpected temporary error: {error:#}"),
    });
    assert_eq!(
        refreshed.access_token,
        std::env::var("TOKS_TEST_CODEX_REFRESH_EXPECTED_ACCESS").unwrap()
    );
}

enum Expected<'a> {
    Access(&'a str),
    NeedsSignIn,
}

fn spawn_helper(
    path: &Path,
    endpoint: &str,
    marker: Option<&Path>,
    expected: Expected<'_>,
) -> Child {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .arg("--exact")
        .arg(HELPER_TEST)
        .arg("--nocapture")
        .env(HELPER_ENV, path)
        .env("TOKS_TEST_CODEX_REFRESH_ENDPOINT", endpoint)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    match expected {
        Expected::Access(access) => {
            command.env("TOKS_TEST_CODEX_REFRESH_EXPECTED_ACCESS", access);
        }
        Expected::NeedsSignIn => {
            command.env("TOKS_TEST_CODEX_REFRESH_EXPECT_SIGN_IN", "1");
        }
    }
    if let Some(marker) = marker {
        command.env("TOKS_TEST_CODEX_REFRESH_MARKER", marker);
    }
    command.spawn().unwrap()
}

fn wait_for_success(mut child: Child) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            panic!("refresh helper timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let mut output = String::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut output)
        .unwrap();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut output)
        .unwrap();
    assert!(status.success(), "refresh helper failed:\n{output}");
}

fn wait_for_file(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !path.is_file() {
        assert!(Instant::now() < deadline, "refresh helper did not start");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn wait_for_open_file(process: u32, expected: &Path) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let opened = std::fs::read_dir(format!("/proc/{process}/fd"))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| std::fs::read_link(entry.path()).ok())
            .any(|path| path == expected);
        if opened {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "refresh helper did not reach the profile lock"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn write_auth(path: &Path, access: &str, refresh: &str) {
    std::fs::write(
        path,
        serde_json::json!({
            "tokens": {
                "access_token": access,
                "refresh_token": refresh,
                "account_id": "synthetic-account"
            }
        })
        .to_string(),
    )
    .unwrap();
}

struct ControlledServer {
    endpoint: String,
    request: mpsc::Receiver<()>,
    release: mpsc::Sender<()>,
    done: Arc<AtomicBool>,
    handle: Option<JoinHandle<usize>>,
}

impl ControlledServer {
    fn new(status: &'static str, body: &'static str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/token", listener.local_addr().unwrap());
        let (request_tx, request) = mpsc::channel();
        let (release, release_rx) = mpsc::channel();
        let done = Arc::new(AtomicBool::new(false));
        let server_done = Arc::clone(&done);
        let handle = std::thread::spawn(move || {
            let (mut first, _) = listener.accept().unwrap();
            read_request(&mut first);
            request_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            write_response(&mut first, status, body);
            listener.set_nonblocking(true).unwrap();
            let mut requests = 1;
            while !server_done.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        read_request(&mut stream);
                        write_response(&mut stream, status, body);
                        requests += 1;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(1));
                    }
                    Err(error) => panic!("OAuth test server failed: {error}"),
                }
            }
            requests
        });
        Self {
            endpoint,
            request,
            release,
            done,
            handle: Some(handle),
        }
    }

    fn wait_for_request(&self) {
        self.request.recv_timeout(Duration::from_secs(5)).unwrap();
    }

    fn release_response(&self) {
        self.release.send(()).unwrap();
    }

    fn finish(mut self) -> usize {
        self.done.store(true, Ordering::Release);
        self.handle.take().unwrap().join().unwrap()
    }
}

impl Drop for ControlledServer {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Release);
    }
}

fn read_request(stream: &mut std::net::TcpStream) {
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
                    .and_then(|value| value.parse::<usize>().ok())
            })
            .unwrap_or(0);
        if bytes.len() >= header_end + 4 + length {
            return;
        }
    }
}

fn write_response(stream: &mut std::net::TcpStream, status: &str, body: &str) {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
}
