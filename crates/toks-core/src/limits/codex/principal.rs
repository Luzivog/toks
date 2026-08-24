use std::path::Path;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde_json::Value;

const PROVIDER_SCOPE: &str = "codex";
const EXPECTED_ISSUER: &str = "https://auth.openai.com";
const ACCOUNT_CLAIM_POINTER: &str = "/https:~1~1api.openai.com~1auth/chatgpt_account_id";
const RS256_SIGNATURE_BYTES: usize = 256;

/// Return provider-scoped identity material from Codex's local ID-token shape.
/// Callers must immediately one-way hash it; raw claims must never be persisted.
/// The provider-owned credential file is the local trust boundary. This parser
/// validates JWT structure but does not cryptographically verify issuer keys.
pub(crate) fn read_principal_material(config_dir: &Path) -> Option<Vec<u8>> {
    let raw = std::fs::read(config_dir.join("auth.json")).ok()?;
    let auth: Value = serde_json::from_slice(&raw).ok()?;
    principal_material_from_auth(&auth)
}

pub(crate) fn principal_material_from_auth(auth: &Value) -> Option<Vec<u8>> {
    let claims = structurally_valid_claims(auth)?;
    let account = account_claim(&claims)?;
    Some(frame(&[
        PROVIDER_SCOPE,
        EXPECTED_ISSUER,
        "account",
        account,
    ]))
}

pub(crate) fn account_header_matches_auth(auth: &Value, account: &str) -> bool {
    structurally_valid_claims(auth)
        .and_then(|claims| account_claim(&claims).map(str::to_owned))
        .is_some_and(|claim| claim == account)
}

fn structurally_valid_claims(auth: &Value) -> Option<Value> {
    let token = auth.pointer("/tokens/id_token")?.as_str()?;
    let (header, claims) = decode_signed_jwt(token)?;
    let algorithm = nonempty(header.get("alg")?.as_str()?)?;
    if algorithm != "RS256" {
        return None;
    }
    let issuer = nonempty(claims.get("iss")?.as_str()?)?;
    (issuer.trim_end_matches('/') == EXPECTED_ISSUER).then_some(claims)
}

fn account_claim(claims: &Value) -> Option<&str> {
    claims
        .pointer(ACCOUNT_CLAIM_POINTER)
        .and_then(Value::as_str)
        .and_then(nonempty)
}

fn decode_signed_jwt(token: &str) -> Option<(Value, Value)> {
    let mut segments = token.split('.');
    let header = segments.next()?;
    let payload = segments.next()?;
    let signature = segments.next()?;
    if segments.next().is_some() || !valid_rs256_signature(signature) {
        return None;
    }
    Some((decode_segment(header)?, decode_segment(payload)?))
}

fn valid_rs256_signature(signature: &str) -> bool {
    let bytes = URL_SAFE_NO_PAD.decode(signature).ok();
    bytes.is_some_and(|bytes| {
        bytes.len() == RS256_SIGNATURE_BYTES && URL_SAFE_NO_PAD.encode(bytes) == signature
    })
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
mod tests;
