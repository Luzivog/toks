use super::*;
use std::io::Write;
use tempfile::NamedTempFile;

fn session_file(content: &str) -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(content.as_bytes()).unwrap();
    file.flush().unwrap();
    file
}

mod contested_matching;
mod lineage_matching;
mod matching_algorithm;
mod parsing_and_dedup;
