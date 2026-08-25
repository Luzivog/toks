use axum::body::Bytes;
use axum::http::header::CONTENT_ENCODING;
use axum::http::{HeaderMap, StatusCode};

use crate::codex_router::proxy::protocol::{rewrite_request, RequestEnvelope};
use crate::rotation::ThreadOverride;

mod compression;
use compression::{decode_zstd, encode_zstd};

pub(super) struct CodexHttpBody {
    wire: Bytes,
    decoded: Bytes,
    encoding: Encoding,
}

pub(super) struct RewrittenBody {
    pub wire: Bytes,
    pub forwarded: String,
    pub automatic_tier_applied: bool,
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

    pub(super) async fn rewrite_request(
        &self,
        request_override: Option<&ThreadOverride>,
        automatic_tier: Option<&str>,
        max_wire_bytes: usize,
    ) -> Result<RewrittenBody, RewriteError> {
        let Some(rewritten) = rewrite_request(
            self.text().unwrap_or_default(),
            RequestEnvelope::HttpResponses,
            request_override,
            automatic_tier,
        ) else {
            return Ok(RewrittenBody {
                wire: self.wire(),
                forwarded: self.text().unwrap_or_default().to_owned(),
                automatic_tier_applied: false,
            });
        };
        if rewritten.payload.as_bytes() == self.decoded.as_ref() {
            return Ok(RewrittenBody {
                wire: self.wire(),
                forwarded: rewritten.payload,
                automatic_tier_applied: rewritten.automatic_tier_applied,
            });
        }
        let forwarded = rewritten.payload;
        let automatic_tier_applied = rewritten.automatic_tier_applied;
        let decoded = Bytes::from(forwarded.clone());
        match self.encoding {
            Encoding::Identity => Ok(RewrittenBody {
                wire: decoded,
                forwarded,
                automatic_tier_applied,
            }),
            Encoding::Zstd => tokio::task::spawn_blocking(move || {
                encode_zstd(decoded, max_wire_bytes).map(|wire| RewrittenBody {
                    wire,
                    forwarded,
                    automatic_tier_applied,
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
