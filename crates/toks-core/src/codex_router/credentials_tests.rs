use super::credentials::token_expiry;

#[test]
fn reads_jwt_expiry_without_trusting_other_claims() {
    let payload = base64::Engine::encode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        br#"{"exp":1893456000,"private":"ignored"}"#,
    );
    let expiry = token_expiry(&format!("header.{payload}.signature")).unwrap();
    assert_eq!(expiry.timestamp(), 1_893_456_000);
}
