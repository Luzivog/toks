use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::UNIX_EPOCH;

use super::identity::hex;
use super::store::SampleDigest;
use super::types::SourceRevision;

const BUFFER_BYTES: usize = 64 * 1024;
const SAMPLE_BYTES: u64 = 4096;
pub(crate) const CODEX_CHUNK_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FileMetadata {
    pub size: u64,
    pub modified_ns: u64,
}

pub(crate) fn metadata(path: &Path) -> Result<FileMetadata, String> {
    let metadata = std::fs::metadata(path).map_err(|error| error.to_string())?;
    let modified_ns = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos().min(u128::from(u64::MAX)) as u64)
        .unwrap_or_default();
    Ok(FileMetadata {
        size: metadata.len(),
        modified_ns,
    })
}

pub(crate) fn complete_codex_boundary(
    path: &Path,
    start: u64,
    source_size: u64,
    target_bytes: u64,
) -> Result<Option<u64>, String> {
    if start >= source_size {
        return Ok(None);
    }
    let target = source_size.min(start.saturating_add(target_bytes.max(1)));
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    file.seek(SeekFrom::Start(start))
        .map_err(|error| error.to_string())?;
    let mut buffer = vec![0_u8; BUFFER_BYTES];
    let mut cursor = start;
    let mut last_complete = None;
    while cursor < source_size {
        let wanted = (source_size - cursor).min(BUFFER_BYTES as u64) as usize;
        let read = file
            .read(&mut buffer[..wanted])
            .map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        for (index, byte) in buffer[..read].iter().enumerate() {
            if *byte != b'\n' {
                continue;
            }
            let boundary = cursor + index as u64 + 1;
            last_complete = Some(boundary);
            if boundary >= target {
                return Ok(Some(boundary));
            }
        }
        cursor += read as u64;
    }
    Ok(last_complete)
}

pub(crate) fn hash_range(path: &Path, start: u64, end: u64) -> Result<[u8; 32], String> {
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    file.seek(SeekFrom::Start(start))
        .map_err(|error| error.to_string())?;
    let mut remaining = end.saturating_sub(start);
    let mut buffer = vec![0_u8; BUFFER_BYTES];
    let mut hasher = Sha256::new();
    while remaining > 0 {
        let wanted = remaining.min(BUFFER_BYTES as u64) as usize;
        let read = file
            .read(&mut buffer[..wanted])
            .map_err(|error| error.to_string())?;
        if read == 0 {
            return Err("accounting source ended during read".to_string());
        }
        hasher.update(&buffer[..read]);
        remaining -= read as u64;
    }
    Ok(hasher.finalize().into())
}

pub(crate) fn prefix_samples(path: &Path, length: u64) -> Result<Vec<SampleDigest>, String> {
    if length == 0 {
        return Ok(Vec::new());
    }
    let len = length.min(SAMPLE_BYTES);
    let mut offsets = vec![
        0,
        length.saturating_sub(len) / 2,
        length.saturating_sub(len),
    ];
    offsets.sort_unstable();
    offsets.dedup();
    offsets
        .into_iter()
        .map(|offset| {
            Ok(SampleDigest {
                offset,
                len,
                hash: hash_range(path, offset, offset + len)?,
            })
        })
        .collect()
}

pub(crate) fn samples_match(path: &Path, samples: &[SampleDigest]) -> bool {
    samples.iter().all(|sample| {
        hash_range(path, sample.offset, sample.offset + sample.len)
            .is_ok_and(|hash| hash == sample.hash)
    })
}

pub(crate) fn revision(parts: &[&[u8]]) -> SourceRevision {
    let mut hasher = Sha256::new();
    hasher.update(b"tokscope.accounting-revision.v1\0");
    for part in parts {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part);
    }
    SourceRevision::new(hex(&hasher.finalize()))
}
