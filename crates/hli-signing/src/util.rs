//! Utilidades comunes de codificación y hashing.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use hli_core::{HliError, Result};
use sha2::{Digest, Sha256};

pub(crate) fn b64_encode(bytes: &[u8]) -> String {
    format!("base64:{}", STANDARD.encode(bytes))
}

pub(crate) fn b64_decode(text: &str) -> Result<Vec<u8>> {
    let raw = text.strip_prefix("base64:").unwrap_or(text);
    STANDARD
        .decode(raw.trim())
        .map_err(|e| HliError::invalid(format!("base64 inválido: {e}")))
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

pub(crate) fn fingerprint(public_key: &[u8]) -> String {
    format!("sha256:{}", sha256_hex(public_key))
}
