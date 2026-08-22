use reqwest::Method;
use serde::{Deserialize, Serialize};

use crate::{
    accounts::AccountProfile,
    limits::{BankedResetAttempt, BankedResetOutcome, LimitIssueKind},
};

use super::super::{
    http::{request_typed_json, LiveError},
    live_fetch::{codex_request_with_method, codex_tokens},
};

const CONSUME_URL: &str = "https://chatgpt.com/backend-api/wham/rate-limit-reset-credits/consume";

#[derive(Serialize)]
struct ConsumeRequest<'a> {
    redeem_request_id: &'a str,
}

#[derive(Deserialize)]
struct ConsumeResponse {
    code: ConsumeCode,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum ConsumeCode {
    Reset,
    NothingToReset,
    NoCredit,
    AlreadyRedeemed,
}

pub(crate) fn redeem_banked_reset(
    profile: &AccountProfile,
    attempt: &BankedResetAttempt,
) -> Result<BankedResetOutcome, LiveError> {
    let (token, account_id) = codex_tokens(profile).ok_or_else(|| {
        LiveError::new(
            LimitIssueKind::Authentication,
            "Codex sign-in is no longer valid",
        )
    })?;
    redeem_with_credentials(&token, account_id.as_deref(), attempt, CONSUME_URL)
}

fn redeem_with_credentials(
    token: &str,
    account_id: Option<&str>,
    attempt: &BankedResetAttempt,
    url: &str,
) -> Result<BankedResetOutcome, LiveError> {
    let response: ConsumeResponse = request_typed_json(|client| {
        codex_request_with_method(client, Method::POST, url, token, account_id).json(
            &ConsumeRequest {
                redeem_request_id: attempt.request_id(),
            },
        )
    })?;
    Ok(match response.code {
        ConsumeCode::Reset => BankedResetOutcome::Reset,
        ConsumeCode::NothingToReset => BankedResetOutcome::NothingToReset,
        ConsumeCode::NoCredit => BankedResetOutcome::NoCredit,
        ConsumeCode::AlreadyRedeemed => BankedResetOutcome::AlreadyRedeemed,
    })
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    use super::{redeem_with_credentials, BankedResetAttempt, BankedResetOutcome};

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
}
