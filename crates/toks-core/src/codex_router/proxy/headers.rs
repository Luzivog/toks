use axum::http::{header, HeaderMap, HeaderName, HeaderValue};

use super::types::RouteCredential;

const ACCOUNT_HEADER: HeaderName = HeaderName::from_static("chatgpt-account-id");

pub(super) fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    (scheme.eq_ignore_ascii_case("bearer") && !token.is_empty()).then_some(token)
}

pub(super) fn upstream_headers(
    incoming: &HeaderMap,
    credential: &RouteCredential,
    websocket: bool,
) -> HeaderMap {
    let mut outgoing = HeaderMap::new();
    for (name, value) in incoming {
        if !excluded(name, websocket) {
            outgoing.append(name.clone(), value.clone());
        }
    }
    let authorization = HeaderValue::from_str(&format!("Bearer {}", credential.access_token))
        .expect("stored access token is a valid HTTP header");
    let account = HeaderValue::from_str(&credential.chatgpt_account_id)
        .expect("stored account ID is a valid HTTP header");
    outgoing.insert(header::AUTHORIZATION, authorization);
    outgoing.insert(ACCOUNT_HEADER, account);
    outgoing
}

pub(super) fn response_headers(incoming: &HeaderMap) -> HeaderMap {
    incoming
        .iter()
        .filter(|(name, _)| !hop_header(name))
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect()
}

fn excluded(name: &HeaderName, websocket: bool) -> bool {
    name == header::AUTHORIZATION
        || name == header::HOST
        || name == ACCOUNT_HEADER
        || hop_header(name)
        || (websocket && name.as_str().starts_with("sec-websocket-"))
}

fn hop_header(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
    )
}
