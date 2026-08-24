use super::{RelatedFileFingerprint, SourceFingerprint};
use crate::message_cache::shard::{compute_sample_hashes, file_fingerprint_parts, ContentHashMode};
use crate::message_cache::CachedPath;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

impl RelatedFileFingerprint {
    pub(super) fn from_path(suffix: String, path: &Path, mode: ContentHashMode) -> Option<Self> {
        let cached_path = CachedPath::from_path(path);
        match path.metadata() {
            Ok(_) => {
                let (size, modified_ns, sample_hashes, content_hash) =
                    file_fingerprint_parts(path, mode)?;
                Some(Self {
                    suffix,
                    path: cached_path,
                    exists: true,
                    size,
                    modified_ns,
                    sample_hashes,
                    content_hash,
                })
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Some(Self {
                suffix,
                path: cached_path,
                exists: false,
                size: 0,
                modified_ns: 0,
                sample_hashes: Vec::new(),
                content_hash: [0; 32],
            }),
            Err(_) => None,
        }
    }
}

pub(super) fn cached_claude_parent_paths(cached: &SourceFingerprint) -> Vec<(String, PathBuf)> {
    cached
        .related_files
        .iter()
        .filter(|related| related.suffix.starts_with("parent-session-"))
        .map(|related| (related.suffix.clone(), related.path.to_path_buf()))
        .collect()
}

pub(super) fn primary_fingerprint_matches(path: &Path, cached: &SourceFingerprint) -> Option<bool> {
    let (size, modified_ns) = metadata_signature(path).ok()?;
    if size != cached.size || modified_ns != cached.modified_ns {
        return Some(false);
    }
    Some(compute_sample_hashes(path, size)? == cached.sample_hashes)
}

pub(in crate::message_cache) fn metadata_signature(path: &Path) -> std::io::Result<(u64, u64)> {
    let metadata = path.metadata()?;
    let modified_ns = metadata
        .modified()?
        .duration_since(UNIX_EPOCH)
        .map_err(std::io::Error::other)?
        .as_nanos() as u64;
    Ok((metadata.len(), modified_ns))
}

pub(super) fn related_fingerprint_metadata_matches(
    related_paths: &[(String, PathBuf)],
    cached: &SourceFingerprint,
) -> Option<bool> {
    if cached.related_files.len() != related_paths.len() {
        return Some(false);
    }

    for (suffix, related_path) in related_paths {
        let Some(related) = cached
            .related_files
            .iter()
            .find(|related| related.suffix == *suffix)
        else {
            return Some(false);
        };
        if related.path != CachedPath::from_path(related_path) {
            return Some(false);
        }
        match metadata_signature(related_path) {
            Ok((size, modified_ns)) => {
                if !related.exists || related.size != size || related.modified_ns != modified_ns {
                    return Some(false);
                }
                if compute_sample_hashes(related_path, size)? != related.sample_hashes {
                    return Some(false);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if related.exists {
                    return Some(false);
                }
            }
            Err(_) => return None,
        }
    }

    Some(true)
}
