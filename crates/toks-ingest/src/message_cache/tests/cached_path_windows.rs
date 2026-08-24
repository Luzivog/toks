use super::*;

/// One file reached under both separators is one cache entry.
///
/// A scan spells a discovered transcript with both: the root half comes
/// from `format!("{root}/{relative}")` and the children below it from
/// `Path::join`. Keying on the raw code units made those two spellings two
/// entries for one file, so the cache could never hit.
#[cfg(windows)]
#[test]
fn cached_path_identity_folds_the_two_windows_separators() {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    fn hash_of(path: &CachedPath) -> u64 {
        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        hasher.finish()
    }

    let mixed = CachedPath::from_path(Path::new(r"C:\home/.claude/projects\demo\s.jsonl"));
    let native = CachedPath::from_path(Path::new(r"C:\home\.claude\projects\demo\s.jsonl"));

    assert_eq!(mixed, native, "both spellings name one file");
    assert_eq!(hash_of(&mixed), hash_of(&native), "Hash must match Eq");

    let mut digests = Vec::new();
    for path in [&mixed, &native] {
        let mut hasher = Sha256::new();
        path.update_digest(&mut hasher);
        digests.push(hasher.finalize());
    }
    assert_eq!(
        digests[0], digests[1],
        "the shard digest must agree too, or one file lands in two shards"
    );

    // The stored spelling is untouched: `to_path_buf` still round-trips,
    // which `SourceMessageCache` relies on to stat the file it cached.
    assert_eq!(
        mixed.to_path_buf(),
        PathBuf::from(r"C:\home/.claude/projects\demo\s.jsonl")
    );

    // Different files stay different.
    let other = CachedPath::from_path(Path::new(r"C:\home\.claude\projects\demo\t.jsonl"));
    assert_ne!(mixed, other);
}

/// After `\\?\` the object manager stops translating, so `/` is an ordinary
/// character in a name rather than a separator. Folding it there would merge
/// two genuinely different paths.
#[cfg(windows)]
#[test]
fn cached_path_identity_leaves_verbatim_paths_alone() {
    let with_slash = CachedPath::from_path(Path::new(r"\\?\C:\dir/name\f.jsonl"));
    let with_backslash = CachedPath::from_path(Path::new(r"\\?\C:\dir\name\f.jsonl"));

    assert_ne!(
        with_slash, with_backslash,
        "inside the verbatim namespace `/` is part of the name, not a separator"
    );
}
