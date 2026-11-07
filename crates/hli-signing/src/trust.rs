//! Trust store local de claves públicas.
//!
//! Una firma criptográficamente válida no es suficiente: la clave debe estar
//! confiada explícitamente aquí y no revocada.

use std::path::{Path, PathBuf};

use hli_core::{time::now_iso, HliError, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::util::{b64_decode, b64_encode, fingerprint, sha256_hex};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrustRecord {
    pub key_id: String,
    pub public_key: String,
    pub fingerprint: String,
    pub label: String,
    pub trusted_at: String,
    pub revoked_at: Option<String>,
}

fn store_path(home: &Path) -> PathBuf {
    home.join("trust.json")
}

fn read_store(home: &Path) -> Result<Vec<TrustRecord>> {
    let path = store_path(home);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = std::fs::read_to_string(&path)?;
    let value: Value = serde_json::from_str(&raw)
        .map_err(|e| HliError::invalid(format!("trust store inválido: {e}")))?;
    Ok(value
        .get("keys")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| serde_json::from_value::<TrustRecord>(x.clone()).ok())
                .collect()
        })
        .unwrap_or_default())
}

fn write_store(home: &Path, keys: &[TrustRecord]) -> Result<()> {
    let body = json!({"keys": keys});
    std::fs::write(store_path(home), serde_json::to_vec_pretty(&body)?)?;
    Ok(())
}

pub fn list(home: &Path) -> Result<Vec<TrustRecord>> {
    read_store(home)
}

pub fn find(home: &Path, key_id: &str) -> Result<Option<TrustRecord>> {
    Ok(read_store(home)?.into_iter().find(|k| k.key_id == key_id))
}

/// Confía en una clave pública en base64 (con o sin prefijo `base64:`).
pub fn add(home: &Path, public_key: &str, label: &str) -> Result<TrustRecord> {
    let bytes = b64_decode(public_key)?;
    let public: [u8; 32] = bytes
        .try_into()
        .map_err(|_| HliError::invalid("la clave pública debe tener 32 bytes"))?;
    let fp = fingerprint(&public);
    let key_id = format!("key_{}", &sha256_hex(&public)[..16]);
    let mut keys = read_store(home)?;
    if let Some(existing) = keys.iter_mut().find(|k| k.key_id == key_id) {
        existing.public_key = b64_encode(&public);
        existing.fingerprint = fp;
        if !label.trim().is_empty() {
            existing.label = label.trim().to_string();
        }
        let out = existing.clone();
        write_store(home, &keys)?;
        return Ok(out);
    }
    let record = TrustRecord {
        key_id: key_id.clone(),
        public_key: b64_encode(&public),
        fingerprint: fp,
        label: if label.trim().is_empty() {
            key_id
        } else {
            label.trim().to_string()
        },
        trusted_at: now_iso(),
        revoked_at: None,
    };
    keys.push(record.clone());
    write_store(home, &keys)?;
    Ok(record)
}

/// Acepta el JSON de `key export-public` o un archivo con la clave en base64.
pub fn add_file(home: &Path, path: &Path) -> Result<TrustRecord> {
    let raw = std::fs::read_to_string(path)?;
    let text = raw.trim();
    if text.starts_with('{') {
        let value: Value = serde_json::from_str(text)
            .map_err(|e| HliError::invalid(format!("clave pública inválida: {e}")))?;
        let public = value
            .get("public_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| HliError::invalid("falta public_key"))?;
        let bytes = b64_decode(public)?;
        let fingerprint_calc = fingerprint(&bytes);
        if let Some(fp) = value.get("fingerprint").and_then(|v| v.as_str()) {
            if fp != fingerprint_calc {
                return Err(HliError::conflict(
                    "el fingerprint no coincide con la clave",
                ));
            }
        }
        if let Some(id) = value.get("key_id").and_then(|v| v.as_str()) {
            let expected = format!("key_{}", &sha256_hex(&bytes)[..16]);
            if id != expected {
                return Err(HliError::conflict("el key_id no coincide con la clave"));
            }
        }
        let label = value
            .get("label")
            .or_else(|| value.get("name"))
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| {
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .map(str::to_string)
            })
            .unwrap_or_default();
        return add(home, public, &label);
    }
    let label = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("imported");
    add(home, text, label)
}

pub fn remove(home: &Path, key_id: &str) -> Result<()> {
    let mut keys = read_store(home)?;
    let before = keys.len();
    keys.retain(|k| k.key_id != key_id);
    if keys.len() == before {
        return Err(HliError::not_found(&format!("clave confiable {key_id}")));
    }
    write_store(home, &keys)
}

pub fn revoke(home: &Path, key_id: &str) -> Result<TrustRecord> {
    let mut keys = read_store(home)?;
    let entry = keys
        .iter_mut()
        .find(|k| k.key_id == key_id)
        .ok_or_else(|| HliError::not_found(&format!("clave confiable {key_id}")))?;
    if entry.revoked_at.is_none() {
        entry.revoked_at = Some(now_iso());
    }
    let out = entry.clone();
    write_store(home, &keys)?;
    Ok(out)
}
