use super::{
    CachedShardEnvelope, ShardReadStatus, CACHE_FORMAT_VERSION, CACHE_SHARD_COUNT,
    MAX_CACHE_SHARD_BYTES,
};
use crate::message_cache::{legacy_wire, CacheIdentity, CacheShardKey, CachedSourceEntry};
use bincode::Options;
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

fn shard_filename(index: usize) -> String {
    format!("shard-{index:02x}.bin")
}

pub(in crate::message_cache) fn parse_shard_filename(filename: &std::ffi::OsStr) -> Option<usize> {
    let filename = filename.to_str()?;
    let encoded = filename.strip_prefix("shard-")?.strip_suffix(".bin")?;
    let index = usize::from_str_radix(encoded, 16).ok()?;
    (index < CACHE_SHARD_COUNT).then_some(index)
}

pub(in crate::message_cache) fn shard_path(root: &Path, key: &CacheShardKey) -> PathBuf {
    root.join(&key.namespace).join(shard_filename(key.index))
}

pub(in crate::message_cache) fn read_shard(
    path: &Path,
    identity: CacheIdentity,
) -> ShardReadStatus {
    read_shard_with_limit(path, identity, MAX_CACHE_SHARD_BYTES)
}

pub(in crate::message_cache) fn read_shard_with_limit(
    path: &Path,
    identity: CacheIdentity,
    max_shard_bytes: u64,
) -> ShardReadStatus {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return ShardReadStatus::Missing
        }
        Err(error) => return ShardReadStatus::Invalid(error.to_string()),
    };
    let metadata = match file.metadata() {
        Ok(metadata) => metadata,
        Err(error) => return ShardReadStatus::Invalid(error.to_string()),
    };
    if metadata.len() > max_shard_bytes {
        return ShardReadStatus::Invalid(format!(
            "{} bytes exceeds the {}-byte shard limit",
            metadata.len(),
            max_shard_bytes
        ));
    }

    let envelope: CachedShardEnvelope = match bincode::options()
        .with_limit(max_shard_bytes)
        .deserialize_from(BufReader::new(file))
    {
        Ok(envelope) => envelope,
        Err(error) => return ShardReadStatus::Invalid(error.to_string()),
    };
    if envelope.parser_namespace != identity.namespace
        || envelope.parser_version != identity.parser_version
    {
        return ShardReadStatus::Stale;
    }

    if let Some(result) =
        legacy_wire::decode(envelope.format_version, &envelope.payload, max_shard_bytes)
    {
        return result
            .map(ShardReadStatus::Migrated)
            .unwrap_or_else(ShardReadStatus::Invalid);
    }
    if envelope.format_version != CACHE_FORMAT_VERSION {
        return ShardReadStatus::Stale;
    }

    match bincode::options()
        .with_limit(max_shard_bytes)
        .deserialize(&envelope.payload)
    {
        Ok(entries) => ShardReadStatus::Loaded(entries),
        Err(error) => ShardReadStatus::Invalid(error.to_string()),
    }
}

pub(in crate::message_cache) fn write_shard_with_limit(
    final_path: &Path,
    identity: CacheIdentity,
    entries: &[CachedSourceEntry],
    max_shard_bytes: u64,
) -> std::io::Result<()> {
    let payload = bincode::options()
        .with_limit(max_shard_bytes)
        .serialize(entries)
        .map_err(std::io::Error::other)?;
    let envelope = CachedShardEnvelope {
        format_version: CACHE_FORMAT_VERSION,
        parser_namespace: identity.namespace.to_string(),
        parser_version: identity.parser_version,
        payload,
    };
    let parent = final_path
        .parent()
        .ok_or_else(|| std::io::Error::other("cache shard has no parent directory"))?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0);
    let tmp_path = parent.join(format!(
        ".{}.{}.{nanos:x}.tmp",
        final_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("source-message-cache"),
        std::process::id(),
    ));

    // INVARIANT: shard writes use atomic temp-file replacement. Never remove
    // the canonical shard before the replacement is completely serialized and
    // fsynced, or one failed large shard write could destroy its last good copy.
    let write_result = (|| -> std::io::Result<()> {
        let file = File::create(&tmp_path)?;
        let mut writer = BufWriter::new(file);
        bincode::options()
            .with_limit(max_shard_bytes)
            .serialize_into(&mut writer, &envelope)
            .map_err(std::io::Error::other)?;
        writer.flush()?;
        writer.get_ref().sync_all()?;
        drop(writer);
        crate::fs_atomic::replace_file(&tmp_path, final_path)?;
        let final_file = OpenOptions::new().read(true).write(true).open(final_path)?;
        final_file.sync_all()?;
        Ok(())
    })();

    if write_result.is_err() {
        let _ = fs::remove_file(&tmp_path);
    }
    write_result
}
