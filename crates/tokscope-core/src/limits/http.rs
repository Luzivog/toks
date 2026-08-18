use std::sync::OnceLock;
use std::time::Duration;

use chrono::{DateTime, Utc};
use reqwest::{Client, RequestBuilder, StatusCode};
use serde_json::Value;

use super::{LimitIssue, LimitIssueKind};

const HTTP_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) struct LiveError {
    pub(crate) issue: LimitIssue,
}

impl LiveError {
    pub(crate) fn new(kind: LimitIssueKind, message: impl Into<String>) -> Self {
        Self {
            issue: LimitIssue::new(kind, message),
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
        if !status.is_success() {
            let kind = match status {
                StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => LimitIssueKind::Authentication,
                StatusCode::TOO_MANY_REQUESTS => LimitIssueKind::RateLimited,
                _ => LimitIssueKind::Network,
            };
            let mut error = LiveError::new(kind, format!("provider returned HTTP {status}"));
            error.issue.retry_at = retry_at;
            return Err(error);
        }
        let body = response.text().await.map_err(|error| {
            LiveError::new(
                LimitIssueKind::Network,
                format!("reading response failed: {error}"),
            )
        })?;
        if body.trim_start().starts_with('<') {
            return Err(LiveError::new(
                LimitIssueKind::Authentication,
                "provider returned a sign-in page",
            ));
        }
        serde_json::from_str(&body).map_err(|error| {
            LiveError::new(
                LimitIssueKind::InvalidResponse,
                format!("provider returned invalid JSON: {error}"),
            )
        })
    })
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
