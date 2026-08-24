use crate::message_cache::FileSampleHash;
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

const FINGERPRINT_SAMPLE_BYTES: usize = 4096;
const FINGERPRINT_SAMPLE_POINTS: usize = 5;
const HASH_BUFFER_BYTES: usize = 64 * 1024;

#[cfg(test)]
thread_local! {
    static FULL_HASH_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn read_sample_hash(file: &mut File, offset: u64, len: usize) -> Option<FileSampleHash> {
    if len == 0 {
        return Some(FileSampleHash {
            offset,
            len: 0,
            hash: 0,
        });
    }

    file.seek(SeekFrom::Start(offset)).ok()?;
    let mut buffer = vec![0_u8; len];
    file.read_exact(&mut buffer).ok()?;

    Some(FileSampleHash {
        offset,
        len: len as u64,
        hash: hash_bytes(&buffer),
    })
}

pub(in crate::message_cache) fn compute_sample_hashes(
    path: &Path,
    size: u64,
) -> Option<Vec<FileSampleHash>> {
    if path.metadata().ok()?.is_dir() {
        return Some(Vec::new());
    }
    if size == 0 {
        return Some(Vec::new());
    }

    let mut file = File::open(path).ok()?;
    let offsets = sample_offsets(size);
    offsets
        .into_iter()
        .map(|(offset, len)| read_sample_hash(&mut file, offset, len))
        .collect()
}

fn sample_offsets(size: u64) -> Vec<(u64, usize)> {
    let sample_len = size.min(FINGERPRINT_SAMPLE_BYTES as u64) as usize;
    if sample_len == 0 {
        return Vec::new();
    }

    let max_offset = size.saturating_sub(sample_len as u64);
    let mut offsets = if max_offset == 0 {
        vec![0]
    } else {
        vec![
            0,
            max_offset / 4,
            max_offset / 2,
            max_offset.saturating_mul(3) / 4,
            max_offset,
        ]
    };
    offsets.sort_unstable();
    offsets.dedup();
    offsets.truncate(FINGERPRINT_SAMPLE_POINTS);
    offsets
        .into_iter()
        .map(|offset| (offset, sample_len))
        .collect()
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Whether a fingerprint carries a whole-file `content_hash`.
///
/// Most warm validation uses size + mtime + samples
/// (`primary_fingerprint_matches` and `related_fingerprint_metadata_matches`).
/// Codex reads `content_hash` for incremental resume, while Prime hashes the
/// complete transcript on every warm hit because its cached messages and
/// reconciliation accounting must describe one exact byte snapshot. Generic
/// parsers and SQLite sources store a zero sentinel so their changed or cold
/// files do not pay for a whole-file hash that cannot affect parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::message_cache) enum ContentHashMode {
    Full,
    SamplesOnly,
}

pub(in crate::message_cache) fn file_fingerprint_parts(
    path: &Path,
    mode: ContentHashMode,
) -> Option<(u64, u64, Vec<FileSampleHash>, [u8; 32])> {
    let metadata = path.metadata().ok()?;
    let size = metadata.len();
    let modified_ns = metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_nanos() as u64;
    let sample_hashes = compute_sample_hashes(path, size)?;
    let content_hash = if metadata.is_dir() {
        [0_u8; 32]
    } else {
        match mode {
            ContentHashMode::Full => hash_prefix(path, size)?,
            ContentHashMode::SamplesOnly => [0_u8; 32],
        }
    };
    Some((size, modified_ns, sample_hashes, content_hash))
}

pub(in crate::message_cache) fn append_path_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut os = OsString::from(path.as_os_str());
    os.push(suffix);
    PathBuf::from(os)
}

pub(in crate::message_cache) fn hash_prefix(path: &Path, len: u64) -> Option<[u8; 32]> {
    #[cfg(test)]
    FULL_HASH_CALLS.with(|calls| calls.set(calls.get() + 1));

    let mut file = File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut remaining = len;
    let mut buffer = [0_u8; HASH_BUFFER_BYTES];

    while remaining > 0 {
        let bytes_to_read = remaining.min(HASH_BUFFER_BYTES as u64) as usize;
        let read = file.read(&mut buffer[..bytes_to_read]).ok()?;
        if read == 0 {
            return None;
        }
        hasher.update(&buffer[..read]);
        remaining -= read as u64;
    }

    Some(hasher.finalize().into())
}

#[cfg(test)]
pub(in crate::message_cache) fn full_hash_call_count() -> usize {
    FULL_HASH_CALLS.with(std::cell::Cell::get)
}
