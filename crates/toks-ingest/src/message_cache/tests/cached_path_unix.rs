use super::*;

#[cfg(unix)]
#[test]
fn test_cached_path_preserves_non_utf8_bytes() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let path = PathBuf::from(OsString::from_vec(vec![0x66, 0x6f, 0x80, 0x6f]));
    let cached_path = CachedPath::from_path(&path);

    assert_eq!(cached_path.to_path_buf(), path);
}
