use std::io;

use super::{StoredCheckpoint, KEY_BYTES};
use crate::accounting_delta::types::SourceKind;

pub(super) fn decode_checkpoint(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredCheckpoint> {
    Ok(StoredCheckpoint {
        kind: decode_kind(row.get(0)?)?,
        parser_version: decode_u32(row.get(1)?)?,
        committed_offset: decode_u64(row.get(2)?)?,
        source_size: decode_u64(row.get(3)?)?,
        modified_ns: decode_u64(row.get(4)?)?,
        content_hash: decode_hash(row.get(5)?)?,
        prefix_samples: decode_json(row.get(6)?)?,
        codex_state: row
            .get::<_, Option<Vec<u8>>>(7)?
            .map(decode_json)
            .transpose()?,
    })
}

pub(super) fn encode_kind(kind: SourceKind) -> i64 {
    match kind {
        SourceKind::Codex => 0,
        SourceKind::Claude => 1,
        SourceKind::OpenCode => 2,
    }
}

pub(super) fn decode_kind(value: i64) -> rusqlite::Result<SourceKind> {
    match value {
        0 => Ok(SourceKind::Codex),
        1 => Ok(SourceKind::Claude),
        2 => Ok(SourceKind::OpenCode),
        _ => Err(invalid_data("invalid source kind")),
    }
}

pub(super) fn encode_u64(value: u64) -> Result<i64, String> {
    i64::try_from(value).map_err(|_| "accounting checkpoint value exceeds SQLite range".to_string())
}

pub(super) fn decode_u64(value: i64) -> rusqlite::Result<u64> {
    u64::try_from(value).map_err(|_| invalid_data("negative accounting checkpoint value"))
}

pub(super) fn decode_u32(value: i64) -> rusqlite::Result<u32> {
    u32::try_from(value).map_err(|_| invalid_data("invalid parser version"))
}

fn decode_hash(bytes: Vec<u8>) -> rusqlite::Result<[u8; KEY_BYTES]> {
    bytes
        .try_into()
        .map_err(|_| invalid_data("invalid content hash"))
}

fn decode_json<T: serde::de::DeserializeOwned>(bytes: Vec<u8>) -> rusqlite::Result<T> {
    serde_json::from_slice(&bytes).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            bytes.len(),
            rusqlite::types::Type::Blob,
            Box::new(error),
        )
    })
}

fn invalid_data(message: &'static str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Integer,
        Box::new(io::Error::new(io::ErrorKind::InvalidData, message)),
    )
}
