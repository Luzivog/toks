use super::*;
use std::io::Write;
use tempfile::NamedTempFile;

mod basic_parsing;
mod cli_attribution;
mod duplicate_identity;
mod record_selection;

fn create_test_file(content: &str) -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(content.as_bytes()).unwrap();
    file.flush().unwrap();
    file
}
