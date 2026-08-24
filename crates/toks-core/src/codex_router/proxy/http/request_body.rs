use axum::body::Bytes;
use axum::http::header::CONTENT_ENCODING;
use axum::http::{HeaderMap, StatusCode};
use serde_json::Value;

mod compression;
use compression::{decode_zstd, encode_zstd};

pub(super) struct CodexHttpBody {
    wire: Bytes,
    decoded: Bytes,
    encoding: Encoding,
}

pub(super) struct RewrittenBody {
    pub wire: Bytes,
    pub forced_fast: bool,
    pub forwarded: String,
}

#[derive(Debug)]
pub(super) enum RewriteError {
    Compression(std::io::Error),
    WorkerStopped(tokio::task::JoinError),
}

impl std::fmt::Display for RewriteError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Compression(error) => write!(formatter, "compressing request body: {error}"),
            Self::WorkerStopped(error) => {
                write!(formatter, "request rewrite worker stopped: {error}")
            }
        }
    }
}

impl std::error::Error for RewriteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Compression(error) => Some(error),
            Self::WorkerStopped(error) => Some(error),
        }
    }
}

#[derive(Clone, Copy)]
enum Encoding {
    Identity,
    Zstd,
}

pub(super) enum DecodeError {
    TooLarge,
    Unsupported,
    Invalid,
}

impl DecodeError {
    pub(super) fn status(&self) -> StatusCode {
        match self {
            Self::TooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            Self::Unsupported => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Self::Invalid => StatusCode::BAD_REQUEST,
        }
    }

    pub(super) fn message(&self) -> &'static str {
        match self {
            Self::TooLarge => "Codex request is too large",
            Self::Unsupported => "Unsupported Codex request encoding",
            Self::Invalid => "Invalid Codex request encoding",
        }
    }
}

impl CodexHttpBody {
    pub(super) async fn decode(
        headers: &HeaderMap,
        wire: Bytes,
        max_decoded_bytes: usize,
    ) -> Result<Self, DecodeError> {
        let encoding = Encoding::from_headers(headers)?;
        let decoded = match encoding {
            Encoding::Identity => wire.clone(),
            Encoding::Zstd => {
                let compressed = wire.clone();
                tokio::task::spawn_blocking(move || decode_zstd(compressed, max_decoded_bytes))
                    .await
                    .map_err(|_| DecodeError::Invalid)??
            }
        };
        Ok(Self {
            wire,
            decoded,
            encoding,
        })
    }

    pub(super) fn decoded(&self) -> &[u8] {
        &self.decoded
    }

    pub(super) fn text(&self) -> Option<&str> {
        std::str::from_utf8(&self.decoded).ok()
    }

    pub(super) fn wire(&self) -> Bytes {
        self.wire.clone()
    }

    pub(super) async fn with_service_tier(
        &self,
        tier: &str,
        is_fast: bool,
        max_wire_bytes: usize,
    ) -> Result<RewrittenBody, RewriteError> {
        let Some(rewritten) = rewrite_service_tier(self.text().unwrap_or_default(), tier) else {
            return Ok(RewrittenBody {
                wire: self.wire(),
                forced_fast: false,
                forwarded: self.text().unwrap_or_default().to_owned(),
            });
        };
        let forced_fast = is_fast && rewritten.as_bytes() != self.decoded.as_ref();
        if rewritten.as_bytes() == self.decoded.as_ref() {
            return Ok(RewrittenBody {
                wire: self.wire(),
                forced_fast,
                forwarded: rewritten,
            });
        }
        let forwarded = rewritten.clone();
        let decoded = Bytes::from(rewritten);
        match self.encoding {
            Encoding::Identity => Ok(RewrittenBody {
                wire: decoded,
                forced_fast,
                forwarded,
            }),
            Encoding::Zstd => tokio::task::spawn_blocking(move || {
                encode_zstd(decoded, max_wire_bytes).map(|wire| RewrittenBody {
                    wire,
                    forced_fast,
                    forwarded,
                })
            })
            .await
            .map_err(RewriteError::WorkerStopped)?
            .map_err(RewriteError::Compression),
        }
    }
}

impl Encoding {
    fn from_headers(headers: &HeaderMap) -> Result<Self, DecodeError> {
        let mut encoding = None;
        for value in headers.get_all(CONTENT_ENCODING) {
            let value = value.to_str().map_err(|_| DecodeError::Unsupported)?;
            for token in value.split(',') {
                let token = token.trim();
                if token.is_empty() || encoding.is_some() {
                    return Err(DecodeError::Unsupported);
                }
                encoding = Some(if token.eq_ignore_ascii_case("identity") {
                    Self::Identity
                } else if token.eq_ignore_ascii_case("zstd") {
                    Self::Zstd
                } else {
                    return Err(DecodeError::Unsupported);
                });
            }
        }
        Ok(encoding.unwrap_or(Self::Identity))
    }
}

fn rewrite_service_tier(payload: &str, tier: &str) -> Option<String> {
    let mut value: Value = serde_json::from_str(payload).ok()?;
    let object = value.as_object_mut()?;
    if object
        .get("type")
        .is_some_and(|kind| kind.as_str() != Some("response.create"))
    {
        return None;
    }
    if object
        .get("service_tier")
        .and_then(Value::as_str)
        .is_some_and(|tier| matches!(tier, "fast" | "priority" | "ultrafast"))
    {
        return Some(payload.to_owned());
    }
    object.insert("service_tier".into(), Value::String(tier.to_owned()));
    serde_json::to_string(&value).ok()
}
