use axum::http::{header, HeaderMap, HeaderName, HeaderValue};

use super::types::RouteCredential;

const ACCOUNT_HEADER: HeaderName = HeaderName::from_static("chatgpt-account-id");
const RESUME_ATTEMPT_HEADER: HeaderName = HeaderName::from_static("x-toks-resume-attempt");
const ACTIVATION_ATTEMPT_HEADER: HeaderName = HeaderName::from_static("x-toks-activation-attempt");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ResumeMarker<'a> {
    Absent,
    Canonical(&'a str),
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ActivationMarker<'a> {
    Absent,
    Canonical(&'a str),
    Invalid,
}

impl<'a> ActivationMarker<'a> {
    pub(super) fn attempt(self) -> Option<&'a str> {
        match self {
            Self::Canonical(attempt) => Some(attempt),
            Self::Absent | Self::Invalid => None,
        }
    }

    pub(super) fn is_present(self) -> bool {
        !matches!(self, Self::Absent)
    }
}

impl<'a> ResumeMarker<'a> {
    pub(super) fn from_attempt(attempt: Option<&'a str>) -> Self {
        attempt.map_or(Self::Absent, Self::Canonical)
    }

    pub(super) fn attempt(self) -> Option<&'a str> {
        match self {
            Self::Canonical(attempt) => Some(attempt),
            Self::Absent | Self::Invalid => None,
        }
    }

    pub(super) fn is_present(self) -> bool {
        !matches!(self, Self::Absent)
    }
}

pub(super) fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    (scheme.eq_ignore_ascii_case("bearer") && !token.is_empty()).then_some(token)
}

pub(super) fn resume_marker(headers: &HeaderMap) -> ResumeMarker<'_> {
    let mut values = headers.get_all(&RESUME_ATTEMPT_HEADER).iter();
    let Some(value) = values.next() else {
        return ResumeMarker::Absent;
    };
    if values.next().is_some() {
        return ResumeMarker::Invalid;
    }
    let Ok(attempt) = value.to_str() else {
        return ResumeMarker::Invalid;
    };
    if uuid::Uuid::parse_str(attempt).is_ok_and(|parsed| parsed.to_string() == attempt) {
        ResumeMarker::Canonical(attempt)
    } else {
        ResumeMarker::Invalid
    }
}

pub(super) fn activation_marker(headers: &HeaderMap) -> ActivationMarker<'_> {
    let mut values = headers.get_all(&ACTIVATION_ATTEMPT_HEADER).iter();
    let Some(value) = values.next() else {
        return ActivationMarker::Absent;
    };
    if values.next().is_some() {
        return ActivationMarker::Invalid;
    }
    let Ok(attempt) = value.to_str() else {
        return ActivationMarker::Invalid;
    };
    if uuid::Uuid::parse_str(attempt).is_ok_and(|parsed| parsed.to_string() == attempt) {
        ActivationMarker::Canonical(attempt)
    } else {
        ActivationMarker::Invalid
    }
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
    if !websocket {
        outgoing.insert(
            header::ACCEPT_ENCODING,
            HeaderValue::from_static("identity"),
        );
    }
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
        || name == header::CONTENT_LENGTH
        || name == header::ACCEPT_ENCODING
        || name == ACCOUNT_HEADER
        || name == RESUME_ATTEMPT_HEADER
        || name == ACTIVATION_ATTEMPT_HEADER
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
