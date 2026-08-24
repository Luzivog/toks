use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// `/` and `\` as UTF-16 code units, and the `\\?\` verbatim prefix.
#[cfg(windows)]
const FORWARD_SLASH_UTF16: u16 = b'/' as u16;
#[cfg(windows)]
const BACKSLASH_UTF16: u16 = b'\\' as u16;
#[cfg(windows)]
const VERBATIM_PREFIX_UTF16: [u16; 4] = [
    BACKSLASH_UTF16,
    BACKSLASH_UTF16,
    b'?' as u16,
    BACKSLASH_UTF16,
];

/// The stored spelling is kept verbatim so [`CachedPath::to_path_buf`] hands
/// back exactly the path that was cached, but *identity* — equality, hashing
/// and the shard digest — folds `/` into `\` first. See [`CachedPath::
/// identity_units`] for why.
#[cfg(windows)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CachedPath(Vec<u16>);

#[cfg(windows)]
impl CachedPath {
    pub(crate) fn from_path(path: &Path) -> Self {
        use std::os::windows::ffi::OsStrExt;

        Self(path.as_os_str().encode_wide().collect())
    }

    pub(crate) fn to_path_buf(&self) -> PathBuf {
        use std::ffi::OsString;
        use std::os::windows::ffi::OsStringExt;

        PathBuf::from(OsString::from_wide(&self.0))
    }

    /// The code units this path is *identified* by: the stored ones, with `/`
    /// folded to `\`.
    ///
    /// On Windows both characters are directory separators, so `C:\a/b\f.jsonl`
    /// and `C:\a\b\f.jsonl` name one file — and a scan produces both spellings
    /// for that one file. `ClientDef::resolve_path` assembles every scan root by
    /// string concatenation (`format!("{root}/{relative}")`), so the root half
    /// carries forward slashes, while `WalkDir` appends each child below it with
    /// the platform separator. Hashing the units as written therefore gave one
    /// file two cache keys.
    ///
    /// That is not only a test artifact. `tokscope --home C:/Users/me` and a
    /// default run (where `dirs` yields `C:\Users\me`) disagree on every key, so
    /// neither run can ever read the other's entries: the cache stays cold and
    /// the shards accumulate a duplicate copy of every file. Git Bash and MSYS2
    /// export `HOME` with forward slashes, so this is reachable without anyone
    /// typing an unusual path.
    ///
    /// Paths in the verbatim namespace are exempt. After `\\?\` the object
    /// manager performs no translation at all, so `/` there is an ordinary
    /// character in a name rather than a separator, and folding it would merge
    /// two genuinely different paths.
    ///
    /// Case is deliberately *not* folded. Windows filesystems are usually but
    /// not always case-insensitive — NTFS supports per-directory sensitivity —
    /// so folding case could merge two real files. Separator folding has no such
    /// exception outside the verbatim namespace, which is why only it is safe.
    fn identity_units(&self) -> impl Iterator<Item = u16> + '_ {
        let verbatim = self.0.starts_with(&VERBATIM_PREFIX_UTF16);
        self.0.iter().map(move |unit| {
            if !verbatim && *unit == FORWARD_SLASH_UTF16 {
                BACKSLASH_UTF16
            } else {
                *unit
            }
        })
    }

    pub(in crate::message_cache) fn update_digest(&self, hasher: &mut Sha256) {
        for code_unit in self.identity_units() {
            hasher.update(code_unit.to_le_bytes());
        }
    }
}

#[cfg(windows)]
impl PartialEq for CachedPath {
    fn eq(&self, other: &Self) -> bool {
        self.identity_units().eq(other.identity_units())
    }
}

#[cfg(windows)]
impl Eq for CachedPath {}

#[cfg(windows)]
impl std::hash::Hash for CachedPath {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Length first, mirroring the `Vec<u16>` derive this replaces. Folding
        // `/` to `\` never changes the length, so this stays consistent with
        // the `PartialEq` above.
        state.write_usize(self.0.len());
        for code_unit in self.identity_units() {
            state.write_u16(code_unit);
        }
    }
}
