use std::path::Path;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde_json::Value;

const PROVIDER_SCOPE: &str = "codex";
const EXPECTED_ISSUER: &str = "https://auth.openai.com";
const ACCOUNT_CLAIM_POINTER: &str = "/https:~1~1api.openai.com~1auth/chatgpt_account_id";

/// Return provider-scoped identity material from Codex's signed ID-token shape.
/// Callers must immediately one-way hash it; raw claims must never be persisted.
pub(crate) fn read_principal_material(config_dir: &Path) -> Option<Vec<u8>> {
    let raw = std::fs::read(config_dir.join("auth.json")).ok()?;
    let auth: Value = serde_json::from_slice(&raw).ok()?;
    let token = auth.pointer("/tokens/id_token")?.as_str()?;
    let (header, claims) = decode_signed_jwt(token)?;

    let algorithm = nonempty(header.get("alg")?.as_str()?)?;
    if algorithm.eq_ignore_ascii_case("none") {
        return None;
    }
    let issuer = nonempty(claims.get("iss")?.as_str()?)?;
    if issuer.trim_end_matches('/') != EXPECTED_ISSUER {
        return None;
    }
    let (kind, subject) = claims
        .pointer(ACCOUNT_CLAIM_POINTER)
        .and_then(Value::as_str)
        .and_then(nonempty)
        .map(|value| ("account", value))
        .or_else(|| {
            claims
                .get("sub")
                .and_then(Value::as_str)
                .and_then(nonempty)
                .map(|value| ("subject", value))
        })?;
    Some(frame(&[PROVIDER_SCOPE, EXPECTED_ISSUER, kind, subject]))
}

fn decode_signed_jwt(token: &str) -> Option<(Value, Value)> {
    let mut segments = token.split('.');
    let header = segments.next()?;
    let payload = segments.next()?;
    let signature = segments.next()?;
    if segments.next().is_some() || signature.is_empty() {
        return None;
    }
    Some((decode_segment(header)?, decode_segment(payload)?))
}

fn decode_segment(segment: &str) -> Option<Value> {
    serde_json::from_slice(&URL_SAFE_NO_PAD.decode(segment).ok()?).ok()
}

fn nonempty(value: &str) -> Option<&str> {
    (!value.trim().is_empty()).then_some(value)
}

fn frame(components: &[&str]) -> Vec<u8> {
    let mut result = Vec::new();
    for component in components {
        let bytes = component.as_bytes();
        let length = u32::try_from(bytes.len()).expect("principal component is bounded");
        result.extend_from_slice(&length.to_be_bytes());
        result.extend_from_slice(bytes);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::{read_principal_material, EXPECTED_ISSUER};
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
    use serde_json::Value;
    use std::path::Path;

    fn write_auth(root: &Path, claims: Value, algorithm: &str, signature: &str) {
        std::fs::create_dir_all(root).unwrap();
        let header = URL_SAFE_NO_PAD.encode(format!(r#"{{"alg":"{algorithm}"}}"#));
        let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap());
        let token = format!("{header}.{payload}.{signature}");
        std::fs::write(
            root.join("auth.json"),
            serde_json::json!({"tokens": {"id_token": token}}).to_string(),
        )
        .unwrap();
    }

    #[test]
    fn stable_account_claim_ignores_email_and_token_signature() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        for (root, email, signature) in [
            (first.path(), "before@example.com", "sig-a"),
            (second.path(), "after@example.com", "sig-b"),
        ] {
            write_auth(
                root,
                serde_json::json!({
                    "iss": EXPECTED_ISSUER,
                    "sub": "user-subject",
                    "https://api.openai.com/auth": {
                        "chatgpt_account_id": "account-subject"
                    },
                    "email": email
                }),
                "RS256",
                signature,
            );
        }
        assert_eq!(
            read_principal_material(first.path()),
            read_principal_material(second.path())
        );
    }

    #[test]
    fn rejects_unsigned_wrong_issuer_and_email_only_tokens() {
        for (claims, algorithm, signature) in [
            (
                serde_json::json!({"iss": EXPECTED_ISSUER, "sub": "subject"}),
                "none",
                "signature",
            ),
            (
                serde_json::json!({"iss": "https://example.com", "sub": "subject"}),
                "RS256",
                "signature",
            ),
            (
                serde_json::json!({"iss": EXPECTED_ISSUER, "email": "same@example.com"}),
                "RS256",
                "signature",
            ),
            (
                serde_json::json!({"iss": EXPECTED_ISSUER, "sub": "subject"}),
                "RS256",
                "",
            ),
        ] {
            let root = tempfile::tempdir().unwrap();
            write_auth(root.path(), claims, algorithm, signature);
            assert!(read_principal_material(root.path()).is_none());
        }
    }

    #[test]
    fn different_provider_accounts_do_not_coalesce() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        for (root, account) in [(first.path(), "account-a"), (second.path(), "account-b")] {
            write_auth(
                root,
                serde_json::json!({
                    "iss": EXPECTED_ISSUER,
                    "sub": "shared-user",
                    "https://api.openai.com/auth": {"chatgpt_account_id": account}
                }),
                "RS256",
                "signature",
            );
        }
        assert_ne!(
            read_principal_material(first.path()),
            read_principal_material(second.path())
        );
    }
}
