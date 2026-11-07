//! Registro local de claves Ed25519.
//!
//! La clave privada vive en `HLI_HOME/keys/<key_id>.key` (base64 del seed) y
//! nunca se serializa en el índice, eventos ni logs. El índice solo contiene
//! metadatos públicos.

use std::path::{Path, PathBuf};

use ed25519_dalek::SigningKey;
use hli_core::{time::now_iso, HliError, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::util::{b64_decode, b64_encode, fingerprint, sha256_hex};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KeyRecord {
    pub key_id: String,
    pub name: String,
    pub public_key: String,
    pub fingerprint: String,
    pub created_at: String,
    pub revoked_at: Option<String>,
}

pub fn keys_dir(home: &Path) -> PathBuf {
    home.join("keys")
}

fn index_path(home: &Path) -> PathBuf {
    keys_dir(home).join("index.json")
}

pub fn private_path(home: &Path, key_id: &str) -> PathBuf {
    keys_dir(home).join(format!("{key_id}.key"))
}

fn read_index(home: &Path) -> Result<Vec<KeyRecord>> {
    let path = index_path(home);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = std::fs::read_to_string(&path)?;
    let value: Value = serde_json::from_str(&raw)
        .map_err(|e| HliError::invalid(format!("índice de claves inválido: {e}")))?;
    let version = value["version"]
        .as_u64()
        .unwrap_or(hli_core::versions::CONFIG_SCHEMA_VERSION as u64);
    if version > hli_core::versions::CONFIG_SCHEMA_VERSION as u64 {
        return Err(HliError::conflict(format!(
            "keys/index.json con versión futura {version}"
        )));
    }
    Ok(value
        .get("keys")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| serde_json::from_value::<KeyRecord>(x.clone()).ok())
                .collect()
        })
        .unwrap_or_default())
}

fn write_index(home: &Path, keys: &[KeyRecord]) -> Result<()> {
    std::fs::create_dir_all(keys_dir(home))?;
    let body = json!({
        "version": hli_core::versions::CONFIG_SCHEMA_VERSION,
        "keys": keys
    });
    std::fs::write(index_path(home), serde_json::to_vec_pretty(&body)?)?;
    Ok(())
}

pub fn list(home: &Path) -> Result<Vec<KeyRecord>> {
    read_index(home)
}

pub fn find(home: &Path, key_id: &str) -> Result<KeyRecord> {
    read_index(home)?
        .into_iter()
        .find(|k| k.key_id == key_id)
        .ok_or_else(|| HliError::not_found(&format!("clave {key_id}")))
}

pub fn generate(home: &Path, name: &str) -> Result<KeyRecord> {
    let name = name.trim();
    if name.is_empty() {
        return Err(HliError::invalid(
            "el nombre de la clave no puede estar vacío",
        ));
    }
    let mut seed = [0u8; 32];
    getrandom::getrandom(&mut seed)
        .map_err(|e| HliError::internal(format!("no se pudo generar entropía: {e}")))?;
    let signing = SigningKey::from_bytes(&seed);
    let public = signing.verifying_key().to_bytes();
    let fp = fingerprint(&public);
    let key_id = format!("key_{}", &sha256_hex(&public)[..16]);
    let record = KeyRecord {
        key_id: key_id.clone(),
        name: name.to_string(),
        public_key: b64_encode(&public),
        fingerprint: fp,
        created_at: now_iso(),
        revoked_at: None,
    };
    std::fs::create_dir_all(keys_dir(home))?;
    let path = private_path(home, &key_id);
    std::fs::write(&path, b64_encode(&seed))?;
    restrict(&path);
    let mut keys = read_index(home)?;
    keys.retain(|k| k.key_id != key_id);
    keys.push(record.clone());
    write_index(home, &keys)?;
    Ok(record)
}

pub fn revoke(home: &Path, key_id: &str) -> Result<KeyRecord> {
    let mut keys = read_index(home)?;
    let entry = keys
        .iter_mut()
        .find(|k| k.key_id == key_id)
        .ok_or_else(|| HliError::not_found(&format!("clave {key_id}")))?;
    if entry.revoked_at.is_none() {
        entry.revoked_at = Some(now_iso());
    }
    let out = entry.clone();
    write_index(home, &keys)?;
    Ok(out)
}

/// Carga la clave privada para firmar. Rechaza claves revocadas.
pub fn signing_key(home: &Path, key_id: &str) -> Result<SigningKey> {
    let record = find(home, key_id)?;
    if record.revoked_at.is_some() {
        return Err(HliError::conflict(format!("clave revocada: {key_id}")));
    }
    let raw = std::fs::read_to_string(private_path(home, key_id))?;
    let seed = b64_decode(&raw)?;
    let seed: [u8; 32] = seed
        .try_into()
        .map_err(|_| HliError::invalid("clave privada corrupta"))?;
    Ok(SigningKey::from_bytes(&seed))
}

pub fn public_bytes(record: &KeyRecord) -> Result<[u8; 32]> {
    b64_decode(&record.public_key)?
        .try_into()
        .map_err(|_| HliError::invalid("clave pública corrupta"))
}

#[cfg(unix)]
fn restrict(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(windows)]
fn restrict(path: &Path) {
    // Restringe la ACL al usuario actual; si falla, el archivo sigue dentro de
    // HLI_HOME y nunca se imprime.
    let user = std::env::var("USERNAME").unwrap_or_default();
    if user.is_empty() {
        return;
    }
    let _ = std::process::Command::new("icacls")
        .arg(path)
        .arg("/inheritance:r")
        .arg("/grant:r")
        .arg(format!("{user}:F"))
        .output();
}
