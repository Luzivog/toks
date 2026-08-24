use super::commandcode::parse_commandcode_file;

#[test]
fn commandcode_empty_assistant_content_has_no_estimated_output_tokens() {
    let temp_dir = tempfile::tempdir().unwrap();
    let project_dir = temp_dir.path().join("projects").join("project");
    std::fs::create_dir_all(&project_dir).unwrap();
    let session_path = project_dir.join("session.jsonl");
    std::fs::write(
        &session_path,
        concat!(
            r#"{"role":"user","sessionId":"session","content":"abcd"}"#,
            "\n",
            r#"{"role":"assistant","sessionId":"session","content":""}"#,
            "\n"
        ),
    )
    .unwrap();

    let messages = parse_commandcode_file(&session_path);

    assert_eq!(messages.len(), 1);
    assert!(messages[0].tokens.input > 0);
    assert_eq!(messages[0].tokens.output, 0);
}
