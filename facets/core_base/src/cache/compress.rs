//! Optional deflate wrap for large Mantle payloads (Phase 3).

use std::io::{Read, Write};

use flate2::Compression;
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;

use super::store::CacheValue;

const MAGIC: &[u8; 4] = b"MZ1\0";

/// Pack bytes when larger than `min_bytes` and compression wins.
pub fn pack(value: CacheValue, min_bytes: usize) -> CacheValue {
    if value.len() < min_bytes {
        return value;
    }
    let Ok(compressed) = deflate(value.as_slice()) else {
        return value;
    };
    if compressed.len().saturating_add(MAGIC.len()) >= value.len() {
        return value;
    }
    let mut out = Vec::with_capacity(MAGIC.len() + compressed.len());
    out.extend_from_slice(MAGIC);
    out.extend(compressed);
    CacheValue::new(out)
}

/// Unpack a value previously passed through [`pack`].
pub fn unpack(value: CacheValue) -> CacheValue {
    let bytes = value.as_slice();
    if bytes.len() < MAGIC.len() || &bytes[..MAGIC.len()] != MAGIC {
        return value;
    }
    match inflate(&bytes[MAGIC.len()..]) {
        Ok(raw) => CacheValue::new(raw),
        Err(_) => value,
    }
}

fn deflate(bytes: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    let mut enc = DeflateEncoder::new(Vec::new(), Compression::fast());
    enc.write_all(bytes)?;
    enc.finish()
}

fn inflate(bytes: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    let mut dec = DeflateDecoder::new(bytes);
    let mut out = Vec::new();
    dec.read_to_end(&mut out)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_large() {
        let raw = CacheValue::new("hello mantle ".repeat(500));
        let packed = pack(raw.clone(), 64);
        assert!(packed.as_slice().starts_with(MAGIC));
        assert!(packed.len() < raw.len());
        let back = unpack(packed);
        assert_eq!(back.as_slice(), raw.as_slice());
    }

    #[test]
    fn small_unchanged() {
        let raw = CacheValue::new("tiny");
        let packed = pack(raw.clone(), 4096);
        assert_eq!(packed.as_slice(), raw.as_slice());
    }
}
