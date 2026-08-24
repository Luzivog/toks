use std::collections::BTreeSet;

use axum::http::HeaderMap;

use crate::rotation::ThreadId;

const THREAD_HEADERS: [&str; 2] = ["thread-id", "x-thread-id"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::codex_router::proxy) enum ThreadIdentity {
    Absent,
    Unique(ThreadId),
    Denied,
}

impl ThreadIdentity {
    pub(in crate::codex_router::proxy) fn from_headers(headers: &HeaderMap) -> Self {
        let mut values = Vec::new();
        for name in THREAD_HEADERS {
            for value in headers.get_all(name) {
                let Ok(value) = value.to_str() else {
                    return Self::Denied;
                };
                values.push(value);
            }
        }
        Self::from_values(values)
    }

    pub(in crate::codex_router::proxy) fn from_payload(payload: &[u8]) -> Self {
        super::request_frame::payload_identity(payload)
    }

    pub(in crate::codex_router::proxy) fn merge(self, other: Self) -> Self {
        match (self, other) {
            (Self::Denied, _) | (_, Self::Denied) => Self::Denied,
            (Self::Absent, identity) | (identity, Self::Absent) => identity,
            (Self::Unique(left), Self::Unique(right)) if left == right => Self::Unique(left),
            (Self::Unique(_), Self::Unique(_)) => Self::Denied,
        }
    }

    pub(in crate::codex_router::proxy) fn into_thread(self) -> Option<ThreadId> {
        match self {
            Self::Unique(thread) => Some(thread),
            Self::Absent | Self::Denied => None,
        }
    }

    pub(super) fn from_values<'a>(values: impl IntoIterator<Item = &'a str>) -> Self {
        let mut unique = BTreeSet::new();
        for value in values {
            if value.is_empty() || value.trim() != value || value.chars().any(char::is_control) {
                return Self::Denied;
            }
            unique.insert(value);
        }
        match unique.len() {
            0 => Self::Absent,
            1 => Self::Unique(ThreadId::new(
                unique.into_iter().next().expect("one identity"),
            )),
            _ => Self::Denied,
        }
    }
}
