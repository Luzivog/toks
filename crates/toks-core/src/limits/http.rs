use std::sync::OnceLock;
use std::time::Duration;

use chrono::{DateTime, Utc};
use reqwest::{Client, RequestBuilder, StatusCode};
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::{LimitIssue, LimitIssueKind};

const HTTP_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug)]
pub(crate) struct LiveError {
    pub(crate) issue: LimitIssue,
    pub(crate) status: Option<StatusCode>,
    pub(crate) error_code: Option<String>,
}

impl LiveError {
    pub(crate) fn new(kind: LimitIssueKind, message: impl Into<String>) -> Self {
        Self {
            issue: LimitIssue::new(kind, message),
            status: None,
            error_code: None,
        }
    }

    fn from_response(kind: LimitIssueKind, status: StatusCode, body: &str) -> Self {
        let parsed = serde_json::from_str::<Value>(body).ok();
        let error_code = parsed.as_ref().and_then(provider_error_code);
        Self {
            issue: LimitIssue::new(kind, format!("provider returned HTTP {status}")),
            status: Some(status),
            error_code,
        }
    }
}

struct HttpExecutor {
    client: Client,
    runtime: tokio::runtime::Runtime,
}

static EXECUTOR: OnceLock<Result<HttpExecutor, String>> = OnceLock::new();

fn executor() -> Result<&'static HttpExecutor, LiveError> {
    EXECUTOR
        .get_or_init(|| {
            let client = Client::builder()
                .timeout(HTTP_TIMEOUT)
                .build()
                .map_err(|error| error.to_string())?;
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .map_err(|error| error.to_string())?;
            Ok(HttpExecutor { client, runtime })
        })
        .as_ref()
        .map_err(|message| LiveError::new(LimitIssueKind::Network, message.clone()))
}

pub(crate) fn get_json(build: impl FnOnce(&Client) -> RequestBuilder) -> Result<Value, LiveError> {
    get_typed_json(build)
}

pub(crate) fn get_typed_json<T: DeserializeOwned>(
    build: impl FnOnce(&Client) -> RequestBuilder,
) -> Result<T, LiveError> {
    let executor = executor()?;
    let request = build(&executor.client);
    executor.runtime.block_on(async move {
        let response = request.send().await.map_err(|error| {
            LiveError::new(
                LimitIssueKind::Network,
                format!("network request failed: {error}"),
            )
        })?;
        let status = response.status();
        let retry_at = retry_at(response.headers().get(reqwest::header::RETRY_AFTER));
        let body = response.text().await.map_err(|error| {
            LiveError::new(
                LimitIssueKind::Network,
                format!("reading response failed: {error}"),
            )
        })?;
        if !status.is_success() {
            let code = serde_json::from_str::<Value>(&body)
                .ok()
                .as_ref()
                .and_then(provider_error_code);
            let kind = match status {
                StatusCode::UNAUTHORIZED => LimitIssueKind::Authentication,
                StatusCode::FORBIDDEN if code.as_deref() == Some("authentication_error") => {
                    LimitIssueKind::Authentication
                }
                StatusCode::TOO_MANY_REQUESTS => LimitIssueKind::RateLimited,
                _ => LimitIssueKind::Network,
            };
            let mut error = LiveError::from_response(kind, status, &body);
            error.issue.retry_at = retry_at;
            return Err(error);
        }
        if body.trim_start().starts_with('<') {
            return Err(LiveError::new(
                LimitIssueKind::InvalidResponse,
                "provider returned HTML instead of JSON",
            ));
        }
        serde_json::from_str::<T>(&body).map_err(|error| {
            LiveError::new(
                LimitIssueKind::InvalidResponse,
                format!("provider returned invalid JSON: {error}"),
            )
        })
    })
}

fn provider_error_code(value: &Value) -> Option<String> {
    value
        .get("error")
        .and_then(|error| {
            error.as_str().or_else(|| {
                error
                    .get("code")
                    .or_else(|| error.get("type"))
                    .and_then(Value::as_str)
            })
        })
        .or_else(|| value.get("code").and_then(Value::as_str))
        .map(str::to_string)
}

fn retry_at(value: Option<&reqwest::header::HeaderValue>) -> Option<DateTime<Utc>> {
    let raw = value?.to_str().ok()?.trim();
    if let Ok(seconds) = raw.parse::<u64>() {
        return chrono::Duration::from_std(Duration::from_secs(seconds))
            .ok()
            .map(|duration| Utc::now() + duration);
    }
    DateTime::parse_from_rfc2822(raw)
        .ok()
        .map(|time| time.with_timezone(&Utc))
}
