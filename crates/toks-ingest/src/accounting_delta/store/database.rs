use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

use super::wire::{
    decode_checkpoint, decode_kind, decode_u32, decode_u64, encode_kind, encode_u64,
};
use super::{CheckpointSummary, SampleDigest, StoredCheckpoint};
use crate::accounting_delta::SourceKey;

pub(super) fn load_checkpoint(
    connection: &Connection,
    source: &SourceKey,
) -> Result<Option<StoredCheckpoint>, String> {
    connection
        .query_row(
            concat!(
                "SELECT kind, parser_version, committed_offset, source_size, modified_ns, ",
                "content_hash, prefix_samples, codex_state FROM sources WHERE source_key = ?1"
            ),
            [source.as_str()],
            decode_checkpoint,
        )
        .optional()
        .map_err(|error| error.to_string())
}

pub(super) fn load_summary(
    connection: &Connection,
    source: &SourceKey,
) -> Result<Option<CheckpointSummary>, String> {
    connection
        .query_row(
            concat!(
                "SELECT kind, parser_version, committed_offset, source_size, modified_ns, ",
                "prefix_samples FROM sources WHERE source_key = ?1"
            ),
            [source.as_str()],
            |row| {
                let samples: Vec<u8> = row.get(5)?;
                Ok(CheckpointSummary {
                    kind: decode_kind(row.get(0)?)?,
                    parser_version: decode_u32(row.get(1)?)?,
                    committed_offset: decode_u64(row.get(2)?)?,
                    source_size: decode_u64(row.get(3)?)?,
                    modified_ns: decode_u64(row.get(4)?)?,
                    prefix_samples: decode_samples(samples)?,
                })
            },
        )
        .optional()
        .map_err(|error| error.to_string())
}

#[cfg(test)]
pub(super) fn commit<'a>(
    connection: &mut Connection,
    checkpoints: impl Iterator<Item = (&'a SourceKey, &'a StoredCheckpoint)>,
) -> Result<(), String> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    for (key, checkpoint) in checkpoints {
        upsert(&transaction, key, checkpoint)?;
    }
    transaction.commit().map_err(|error| error.to_string())
}

pub(super) fn acknowledge(
    connection: &mut Connection,
    key: &SourceKey,
    checkpoint: Option<&StoredCheckpoint>,
) -> Result<(), String> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    if let Some(checkpoint) = checkpoint {
        upsert(&transaction, key, checkpoint)?;
    }
    transaction
        .execute(
            "UPDATE meta SET rotation_cursor = ?1 WHERE singleton = 1 AND rotation_cursor IS NOT ?1",
            [key.as_str()],
        )
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())
}

pub(super) fn upsert(
    connection: &Connection,
    key: &SourceKey,
    checkpoint: &StoredCheckpoint,
) -> Result<(), String> {
    let prefix_samples =
        serde_json::to_vec(&checkpoint.prefix_samples).map_err(|error| error.to_string())?;
    let codex_state = checkpoint
        .codex_state
        .as_ref()
        .map(serde_json::to_vec)
        .transpose()
        .map_err(|error| error.to_string())?;
    connection
        .execute(
            concat!(
                "INSERT INTO sources VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) ",
                "ON CONFLICT(source_key) DO UPDATE SET kind=?2, parser_version=?3, ",
                "committed_offset=?4, source_size=?5, modified_ns=?6, content_hash=?7, ",
                "prefix_samples=?8, codex_state=?9 WHERE kind IS NOT ?2 OR parser_version IS NOT ?3 ",
                "OR committed_offset IS NOT ?4 OR source_size IS NOT ?5 OR modified_ns IS NOT ?6 ",
                "OR content_hash IS NOT ?7 OR prefix_samples IS NOT ?8 OR codex_state IS NOT ?9"
            ),
            params![
                key.as_str(),
                encode_kind(checkpoint.kind),
                i64::from(checkpoint.parser_version),
                encode_u64(checkpoint.committed_offset)?,
                encode_u64(checkpoint.source_size)?,
                encode_u64(checkpoint.modified_ns)?,
                checkpoint.content_hash.as_slice(),
                prefix_samples,
                codex_state,
            ],
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn decode_samples(bytes: Vec<u8>) -> rusqlite::Result<Vec<SampleDigest>> {
    serde_json::from_slice(&bytes).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            bytes.len(),
            rusqlite::types::Type::Blob,
            Box::new(error),
        )
    })
}
