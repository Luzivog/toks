use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};

use super::redeem::redeem_with_credentials;
use crate::limits::{BankedResetAttempt, BankedResetOutcome};

#[test]
fn redemption_contract_uses_only_the_injected_loopback_endpoint() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("loopback test listener binds");
    let url = format!("http://{}/consume", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_request(&mut stream);
        let body = br#"{"code":"reset","windows_reset":2}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .unwrap();
        stream.write_all(body).unwrap();
        request
    });
    let attempt = BankedResetAttempt::new();

    let outcome =
        redeem_with_credentials("synthetic-token", Some("synthetic-account"), &attempt, &url)
            .unwrap();

    assert_eq!(outcome, BankedResetOutcome::Reset);
    let request = server.join().unwrap();
    assert!(request.starts_with("POST /consume HTTP/1.1\r\n"));
    assert!(request.contains("authorization: Bearer synthetic-token\r\n"));
    assert!(request.contains("chatgpt-account-id: synthetic-account\r\n"));
    assert!(request.ends_with(&format!(
        "{{\"redeem_request_id\":\"{}\"}}",
        attempt.request_id()
    )));
}

fn read_request(stream: &mut std::net::TcpStream) -> String {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 1024];
    loop {
        let read = stream.read(&mut buffer).unwrap();
        request.extend_from_slice(&buffer[..read]);
        let header_end = request
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .map(|index| index + 4);
        let Some(header_end) = header_end else {
            continue;
        };
        let headers = String::from_utf8_lossy(&request[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                line.to_ascii_lowercase()
                    .strip_prefix("content-length: ")
                    .and_then(|value| value.parse::<usize>().ok())
            })
            .unwrap_or(0);
        if request.len() >= header_end + content_length {
            return String::from_utf8(request).unwrap();
        }
    }
}
