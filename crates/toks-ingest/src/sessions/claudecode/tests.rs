use super::*;
use crate::paths::json_path_literal;
use std::io::Write;
use tempfile::{NamedTempFile, TempDir};

mod dedup;
mod human_turns;
mod providers_and_headless;
mod sidechains;
mod subagent_resolution;
mod tool_results;
mod transcripts;
mod workflows;

fn create_test_file(content: &str) -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(content.as_bytes()).unwrap();
    file.flush().unwrap();
    file
}

fn create_project_file(
    content: &str,
    project: &str,
    filename: &str,
) -> (TempDir, std::path::PathBuf) {
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir
        .path()
        .join(".claude")
        .join("projects")
        .join(project)
        .join(filename);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, content).unwrap();
    (temp_dir, path)
}

fn create_cc_mirror_project_file(
    content: &str,
    variant: &str,
    provider: &str,
    project: &str,
    filename: &str,
) -> (TempDir, std::path::PathBuf) {
    let temp_dir = tempfile::tempdir().unwrap();
    let variant_dir = temp_dir.path().join(".cc-mirror").join(variant);
    let config_dir = variant_dir.join("config");
    let path = config_dir.join("projects").join(project).join(filename);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        variant_dir.join("variant.json"),
        format!(
            r#"{{"name":"{variant}","provider":"{provider}","configDir":{}}}"#,
            json_path_literal(&config_dir)
        ),
    )
    .unwrap();
    std::fs::write(&path, content).unwrap();
    (temp_dir, path)
}
