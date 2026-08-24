use std::io::{Cursor, Read, Write};

use axum::body::Bytes;

use super::DecodeError;

pub(super) fn decode_zstd(wire: Bytes, limit: usize) -> Result<Bytes, DecodeError> {
    let decoder =
        zstd::stream::read::Decoder::new(Cursor::new(wire)).map_err(|_| DecodeError::Invalid)?;
    let mut decoded = Vec::new();
    decoder
        .take(limit.saturating_add(1) as u64)
        .read_to_end(&mut decoded)
        .map_err(|_| DecodeError::Invalid)?;
    if decoded.len() > limit {
        return Err(DecodeError::TooLarge);
    }
    Ok(decoded.into())
}

pub(super) fn encode_zstd(decoded: Bytes, limit: usize) -> Result<Bytes, ()> {
    let output = BoundedOutput::new(limit);
    let mut encoder = zstd::stream::write::Encoder::new(output, 3).map_err(|_| ())?;
    encoder.write_all(&decoded).map_err(|_| ())?;
    let output = encoder.finish().map_err(|_| ())?;
    Ok(output.bytes.into())
}

struct BoundedOutput {
    bytes: Vec<u8>,
    limit: usize,
}

impl BoundedOutput {
    fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::new(),
            limit,
        }
    }
}

impl Write for BoundedOutput {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        if buffer.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(std::io::Error::other("encoded request exceeds limit"));
        }
        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
