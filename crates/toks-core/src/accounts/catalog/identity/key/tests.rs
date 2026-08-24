use std::sync::{Arc, Barrier};

use super::{load_or_create_at, KEY_BYTES};

#[test]
fn malformed_first_run_file_is_repaired_atomically() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("account-principal.key");
    std::fs::write(&path, [9_u8; 7]).unwrap();

    let key = load_or_create_at(&path).unwrap();

    assert_eq!(key.len(), KEY_BYTES);
    assert_eq!(std::fs::read(path).unwrap(), key);
}

#[test]
fn concurrent_first_run_readers_share_one_fully_published_key() {
    const READERS: usize = 16;
    let directory = tempfile::tempdir().unwrap();
    let path = Arc::new(directory.path().join("account-principal.key"));
    let start = Arc::new(Barrier::new(READERS));
    let readers = (0..READERS)
        .map(|_| {
            let path = Arc::clone(&path);
            let start = Arc::clone(&start);
            std::thread::spawn(move || {
                start.wait();
                load_or_create_at(&path).unwrap()
            })
        })
        .collect::<Vec<_>>();
    let keys = readers
        .into_iter()
        .map(|reader| reader.join().unwrap())
        .collect::<Vec<_>>();

    assert!(keys.iter().all(|key| key == &keys[0]));
    assert_eq!(keys[0].len(), KEY_BYTES);
    assert_eq!(std::fs::read(path.as_ref()).unwrap(), keys[0]);
}
