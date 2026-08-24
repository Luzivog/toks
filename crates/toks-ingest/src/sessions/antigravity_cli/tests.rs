use super::*;
use rusqlite::{params, Connection};

mod database_parsing;
mod model_recovery;
mod usage_attribution;
mod wire_and_paths;

fn enc_varint(field: u64, value: u64) -> Vec<u8> {
    let mut out = encode_varint(field << 3);
    out.extend(encode_varint(value));
    out
}

fn enc_len(field: u64, payload: &[u8]) -> Vec<u8> {
    let mut out = encode_varint((field << 3) | 2);
    out.extend(encode_varint(payload.len() as u64));
    out.extend_from_slice(payload);
    out
}

fn encode_varint(mut value: u64) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
    out
}

/// Parse one row with no conversation-level attribution available, i.e. as
/// if it were the file's only row. Rows that carry their own `#19` are
/// unaffected by the session index, so most tests need nothing else.
fn parse_isolated_row(
    blob: &[u8],
    session_id: &str,
    session_timestamp: i64,
    seen_response_ids: &mut HashSet<String>,
) -> Option<UnifiedMessage> {
    parse_gen_metadata(
        blob,
        session_id,
        session_timestamp,
        &SessionModels::default(),
        seen_response_ids,
    )
}

fn build_gen_metadata() -> Vec<u8> {
    build_gen_metadata_with_model("gemini-3-flash-a")
}

fn build_gen_metadata_with_model(model: &str) -> Vec<u8> {
    build_row(Some(model), None, "resp-1")
}

/// One `gen_metadata` blob with either model field independently present or
/// absent, mirroring the real rows where `#19` is missing but `#21` is not.
fn build_row(model: Option<&str>, display: Option<&str>, response_id: &str) -> Vec<u8> {
    // usage message (#4 of chatModel)
    let mut usage = Vec::new();
    usage.extend(enc_varint(1, 1132)); // fixed system prompt
    usage.extend(enc_varint(2, 500)); // new input
    usage.extend(enc_varint(5, 16000)); // cacheRead
    usage.extend(enc_varint(9, 300)); // output
    usage.extend(enc_varint(10, 40)); // thinking
    usage.extend(enc_len(11, response_id.as_bytes())); // responseId

    // chatModel message (#1 of gen_metadata)
    let mut chat_model = Vec::new();
    chat_model.extend(enc_len(4, &usage));
    if let Some(model) = model {
        chat_model.extend(enc_len(19, model.as_bytes()));
    }
    if let Some(display) = display {
        chat_model.extend(enc_len(21, display.as_bytes()));
    }

    enc_len(1, &chat_model)
}
