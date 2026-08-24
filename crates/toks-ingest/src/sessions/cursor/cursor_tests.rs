use super::*;

#[test]
fn test_infer_provider() {
    let provider_for = |model| provider_from_model_or(model, "cursor");
    assert_eq!(provider_for("claude-3-sonnet"), "anthropic");
    assert_eq!(provider_for("gpt-4o"), "openai");
    assert_eq!(provider_for("gemini-pro"), "google");
    assert_eq!(provider_for("deepseek-coder"), "deepseek");
    assert_eq!(provider_for("llama-3"), "meta");
    assert_eq!(provider_for("unknown-model"), "cursor");
}

#[test]
fn test_parse_cost() {
    assert_eq!(parse_cost("$0.50"), 0.50);
    assert_eq!(parse_cost("0.50"), 0.50);
    assert_eq!(parse_cost("$1,234.56"), 1234.56);
    assert_eq!(parse_cost(""), 0.0);
    assert_eq!(parse_cost("NaN"), 0.0);
    assert_eq!(parse_cost("nan"), 0.0);
    assert_eq!(parse_cost("  "), 0.0);
    // v3 format values
    assert_eq!(parse_cost("Included"), 0.0);
    assert_eq!(parse_cost("-"), 0.0);
}

#[test]
fn test_parse_csv_line() {
    let line = "2025-02-01,gpt-4o,10,5,0,15,30,$0.10,$0.10";
    let fields = parse_csv_line(line);
    assert_eq!(fields.len(), 9);
    assert_eq!(fields[0], "2025-02-01");
    assert_eq!(fields[1], "gpt-4o");
    assert_eq!(fields[8], "$0.10");
}

#[test]
fn test_parse_date_to_timestamp() {
    // ISO with milliseconds and Z (new Cursor format)
    let ts = parse_date_to_timestamp("2025-11-13T18:36:05.846Z");
    assert!(ts > 0);

    // ISO with Z
    let ts = parse_date_to_timestamp("2025-02-05T12:00:00Z");
    assert!(ts > 0);

    // Date only
    let ts = parse_date_to_timestamp("2025-02-05");
    assert!(ts > 0);

    // Invalid
    let ts = parse_date_to_timestamp("invalid");
    assert_eq!(ts, 0);
}

#[test]
fn test_parse_cursor_csv_sample_old_format() {
    let csv = "Date,Model,Input (w/ Cache Write),Input (w/o Cache Write),Cache Read,Output Tokens,Total Tokens,Cost,Cost to you
2025-02-01,gpt-4o,10,5,0,15,30,$0.10,$0.10
2025-02-02,gpt-4o-mini,0,0,0,5,5,$0.05,$0.05";

    let temp_dir = tempfile::TempDir::new().unwrap();
    let file_path = temp_dir.path().join("usage.csv");
    std::fs::write(&file_path, csv).unwrap();

    let messages = parse_cursor_file(&file_path);
    assert_eq!(messages.len(), 2);

    assert_eq!(messages[0].client, "cursor");
    assert_eq!(messages[0].model_id, "gpt-4o");
    assert_eq!(messages[0].provider_id, "openai");
    assert_eq!(messages[0].tokens.input, 5);
    assert_eq!(messages[0].tokens.output, 15);
    assert_eq!(messages[0].tokens.cache_write, 5); // 10 - 5
    assert!((messages[0].cost - 0.10).abs() < 0.001);

    assert_eq!(messages[1].model_id, "gpt-4o-mini");
}

#[test]
fn test_parse_cursor_csv_sample_new_format() {
    // Real format from Cursor API
    let csv = r#"Date,Kind,Model,Max Mode,Input (w/ Cache Write),Input (w/o Cache Write),Cache Read,Output Tokens,Total Tokens,Cost
"2025-11-13T18:36:05.846Z","Included","auto","No","28342","775","105891","21282","156290","0.19"
"2025-11-13T13:35:04.658Z","On-Demand","gpt-5-codex","No","0","8263","66964","1612","76839","0.03""#;

    let temp_dir = tempfile::TempDir::new().unwrap();
    let file_path = temp_dir.path().join("usage.csv");
    std::fs::write(&file_path, csv).unwrap();

    let messages = parse_cursor_file(&file_path);
    assert_eq!(messages.len(), 2);

    // First message: auto model
    assert_eq!(messages[0].client, "cursor");
    assert_eq!(messages[0].model_id, "auto");
    assert_eq!(messages[0].provider_id, "cursor"); // unknown model -> cursor
    assert_eq!(messages[0].tokens.input, 775);
    assert_eq!(messages[0].tokens.output, 21282);
    assert_eq!(messages[0].tokens.cache_read, 105891);
    assert_eq!(messages[0].tokens.cache_write, 28342 - 775); // 27567
    assert!((messages[0].cost - 0.19).abs() < 0.001);

    // Second message: gpt-5-codex
    assert_eq!(messages[1].model_id, "gpt-5-codex");
    assert_eq!(messages[1].provider_id, "openai"); // gpt -> openai
    assert_eq!(messages[1].tokens.input, 8263);
    assert_eq!(messages[1].tokens.cache_read, 66964);
}

#[test]
fn test_parse_cursor_csv_sample_v3_format() {
    // v3 format includes Cloud Agent ID and Automation ID columns
    let csv = r#"Date,Cloud Agent ID,Automation ID,Kind,Model,Max Mode,Input (w/ Cache Write),Input (w/o Cache Write),Cache Read,Output Tokens,Total Tokens,Cost
"2026-04-09T20:01:10.528Z","bc-a380fb49-e1a5-414e-817d-6a85b6cdc51c","cc30782e-26cc-4359-bc22-7567efe282be","Included","composer-2","Yes","0","343446","29045760","915201","30304407","Included"
"2026-04-09T18:02:13.576Z","bc-19a9b74b-2af3-46e2-9f61-3ba1cdac46c8","1a0df38f-1474-4dfe-896b-70b841d4a833","On-Demand","composer-2","Yes","0","43478","420864","7957","472299","0.11"
"2026-04-09T07:39:09.091Z","bc-49262501-0ee0-49f9-b856-a5b0466deddb","","Errored, No Charge","composer-2","Yes","0","104504","985600","3666","1093770","-""#;

    let temp_dir = tempfile::TempDir::new().unwrap();
    let file_path = temp_dir.path().join("usage.csv");
    std::fs::write(&file_path, csv).unwrap();

    let messages = parse_cursor_file(&file_path);
    assert_eq!(messages.len(), 3);

    // First message: "Included" cost should be 0
    assert_eq!(messages[0].client, "cursor");
    assert_eq!(messages[0].model_id, "composer-2");
    assert_eq!(messages[0].cost, 0.0);
    assert_eq!(messages[0].tokens.cache_read, 29045760);

    // Second message: actual cost from "On-Demand"
    assert_eq!(messages[1].model_id, "composer-2");
    assert!((messages[1].cost - 0.11).abs() < 0.001);

    // Third message: "-" cost should be 0 (Errored, No Charge)
    assert_eq!(messages[2].model_id, "composer-2");
    assert_eq!(messages[2].cost, 0.0);
}
