//! Payload canónico, firma y verificación de contenedores `.hlimod`.

use std::path::Path;

use ed25519_dalek::{Signature, Signer, Verifier, VerifyingKey};
use hli_core::{canon::canonical_json, time::now_iso, HliError, Result};
use hli_store::export::{read_container_limited, ArchiveFile, HLIMOD_LIMITS};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::util::{b64_decode, b64_encode, fingerprint, sha256_hex};
use crate::{keys, trust, SignatureState};

pub const SIGNATURE_FILE: &str = "signature.json";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SignatureFile {
    pub algorithm: String,
    pub key_id: String,
    pub public_key_fingerprint: String,
    pub signed_at: String,
    pub payload_hash: String,
    pub signature: String,
}

#[derive(Clone, Debug)]
pub struct Verification {
    pub state: SignatureState,
    pub key_id: Option<String>,
    pub fingerprint: Option<String>,
    pub payload_hash: Option<String>,
    pub message: String,
}

impl Verification {
    fn new(
        state: SignatureState,
        key_id: Option<String>,
        fingerprint: Option<String>,
        payload_hash: Option<String>,
        message: &str,
    ) -> Self {
        Self {
            state,
            key_id,
            fingerprint,
            payload_hash,
            message: message.to_string(),
        }
    }

    pub fn to_json(&self) -> Value {
        json!({
            "state": self.state.as_str(),
            "key_id": self.key_id,
            "fingerprint": self.fingerprint,
            "payload_hash": self.payload_hash,
            "message": self.message,
        })
    }
}

fn manifest_of(files: &[ArchiveFile]) -> Result<Value> {
    let entry = files
        .iter()
        .find(|f| f.path == "manifest.json")
        .ok_or_else(|| HliError::invalid("paquete sin manifest.json"))?;
    serde_json::from_slice(&entry.bytes)
        .map_err(|e| HliError::invalid(format!("manifest inválido: {e}")))
}

fn get<'a>(files: &'a [ArchiveFile], path: &str) -> Option<&'a [u8]> {
    files
        .iter()
        .find(|f| f.path == path)
        .map(|f| f.bytes.as_slice())
}

/// Valida presencia y hashes de `checksums.json` (manifest y entry incluidos).
pub fn check_checksums(files: &[ArchiveFile]) -> Result<()> {
    let manifest = manifest_of(files)?;
    let checks: Value = serde_json::from_slice(
        get(files, "checksums.json")
            .ok_or_else(|| HliError::invalid("paquete sin checksums.json"))?,
    )
    .map_err(|e| HliError::invalid(format!("checksums.json inválido: {e}")))?;
    let checks = checks
        .as_object()
        .ok_or_else(|| HliError::invalid("checksums inválido"))?;
    if !checks.contains_key("manifest.json") {
        return Err(HliError::invalid("checksums no cubre manifest.json"));
    }
    let entry = manifest["spec"]["runtime"]["entry"].as_str().unwrap_or("");
    if !entry.is_empty() && !checks.contains_key(entry) {
        return Err(HliError::invalid(format!(
            "checksums no cubre el binario {entry}"
        )));
    }
    for (path, expected) in checks {
        let bytes = get(files, path)
            .ok_or_else(|| HliError::invalid(format!("falta {path} en el paquete")))?;
        let got = format!("sha256:{}", sha256_hex(bytes));
        if got != expected.as_str().unwrap_or("") {
            return Err(HliError::conflict(format!("checksum no coincide: {path}")));
        }
    }
    Ok(())
}

/// Manifiesto canónico firmado: formato, archivos ordenados con tamaño y hash,
/// y migraciones declaradas en el manifest. No incluye `signature.json`.
pub fn canonical_payload(files: &[ArchiveFile]) -> Result<String> {
    let manifest = manifest_of(files)?;
    let migrations = manifest
        .get("spec")
        .and_then(|s| s.get("migrations"))
        .cloned()
        .unwrap_or(Value::Null);
    let mut entries: Vec<&ArchiveFile> =
        files.iter().filter(|f| f.path != SIGNATURE_FILE).collect();
    entries.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));
    let list: Vec<Value> = entries
        .iter()
        .map(|f| {
            json!({
                "path": f.path,
                "sha256": sha256_hex(&f.bytes),
                "size": f.bytes.len() as u64
            })
        })
        .collect();
    Ok(canonical_json(&json!({
        "format": "hli-signing/v1",
        "files": list,
        "migrations": migrations
    })))
}

pub fn payload_hash(files: &[ArchiveFile]) -> Result<String> {
    Ok(format!(
        "sha256:{}",
        sha256_hex(canonical_payload(files)?.as_bytes())
    ))
}

pub fn read_package(path: &Path) -> Result<Vec<ArchiveFile>> {
    let raw = std::fs::read(path)?;
    read_container_limited(&raw, HLIMOD_LIMITS)
}

pub fn write_package(path: &Path, files: &[ArchiveFile]) -> Result<()> {
    std::fs::write(path, hli_store::export::container_bytes(files))?;
    Ok(())
}

pub fn sign_files(
    files: &mut Vec<ArchiveFile>,
    home: &Path,
    key_id: &str,
) -> Result<SignatureFile> {
    check_checksums(files)?;
    let payload = canonical_payload(files)?;
    let record = keys::find(home, key_id)?;
    let signing = keys::signing_key(home, key_id)?;
    let signature = signing.sign(payload.as_bytes());
    let file = SignatureFile {
        algorithm: "ed25519".into(),
        key_id: record.key_id,
        public_key_fingerprint: record.fingerprint,
        signed_at: now_iso(),
        payload_hash: format!("sha256:{}", sha256_hex(payload.as_bytes())),
        signature: b64_encode(&signature.to_bytes()),
    };
    files.retain(|f| f.path != SIGNATURE_FILE);
    files.push(ArchiveFile {
        path: SIGNATURE_FILE.into(),
        bytes: serde_json::to_vec(&file)?,
    });
    Ok(file)
}

pub fn sign_package(path: &Path, home: &Path, key_id: &str) -> Result<SignatureFile> {
    let mut files = read_package(path)?;
    let sig = sign_files(&mut files, home, key_id)?;
    write_package(path, &files)?;
    Ok(sig)
}

pub fn verify_files(files: &[ArchiveFile], home: &Path) -> Result<Verification> {
    check_checksums(files)?;
    let Some(raw_sig) = get(files, SIGNATURE_FILE) else {
        return Ok(Verification::new(
            SignatureState::Unsigned,
            None,
            None,
            None,
            "paquete sin firma",
        ));
    };
    let sig: SignatureFile = serde_json::from_slice(raw_sig)
        .map_err(|e| HliError::invalid(format!("signature.json inválido: {e}")))?;
    if !sig.algorithm.eq_ignore_ascii_case("ed25519") {
        return Ok(Verification::new(
            SignatureState::InvalidSignature,
            Some(sig.key_id),
            Some(sig.public_key_fingerprint),
            None,
            "algoritmo de firma no soportado",
        ));
    }
    let payload = canonical_payload(files)?;
    let computed = format!("sha256:{}", sha256_hex(payload.as_bytes()));
    if computed != sig.payload_hash {
        return Ok(Verification::new(
            SignatureState::InvalidSignature,
            Some(sig.key_id),
            Some(sig.public_key_fingerprint),
            Some(computed),
            "el payload no coincide con el hash firmado",
        ));
    }
    let Some(trusted) = trust::find(home, &sig.key_id)? else {
        return Ok(Verification::new(
            SignatureState::SignedUntrusted,
            Some(sig.key_id),
            Some(sig.public_key_fingerprint),
            Some(computed),
            "la clave no está en el trust store",
        ));
    };
    if trusted.fingerprint != sig.public_key_fingerprint {
        return Ok(Verification::new(
            SignatureState::InvalidSignature,
            Some(sig.key_id),
            Some(sig.public_key_fingerprint),
            Some(computed),
            "el fingerprint de la firma no coincide con la clave confiada",
        ));
    }
    let public: [u8; 32] = match b64_decode(&trusted.public_key) {
        Ok(v) => match v.try_into() {
            Ok(pk) => pk,
            Err(_) => {
                return Ok(Verification::new(
                    SignatureState::InvalidSignature,
                    Some(sig.key_id),
                    Some(sig.public_key_fingerprint),
                    Some(computed),
                    "clave pública confiada corrupta",
                ))
            }
        },
        Err(_) => {
            return Ok(Verification::new(
                SignatureState::InvalidSignature,
                Some(sig.key_id),
                Some(sig.public_key_fingerprint),
                Some(computed),
                "clave pública confiada corrupta",
            ))
        }
    };
    if fingerprint(&public) != trusted.fingerprint {
        return Ok(Verification::new(
            SignatureState::InvalidSignature,
            Some(sig.key_id),
            Some(sig.public_key_fingerprint),
            Some(computed),
            "el fingerprint no corresponde a la clave pública",
        ));
    }
    let verifying = match VerifyingKey::from_bytes(&public) {
        Ok(v) => v,
        Err(_) => {
            return Ok(Verification::new(
                SignatureState::InvalidSignature,
                Some(sig.key_id),
                Some(sig.public_key_fingerprint),
                Some(computed),
                "clave pública inválida",
            ))
        }
    };
    let signature_bytes = match b64_decode(&sig.signature) {
        Ok(v) => v,
        Err(_) => {
            return Ok(Verification::new(
                SignatureState::InvalidSignature,
                Some(sig.key_id),
                Some(sig.public_key_fingerprint),
                Some(computed),
                "firma no es base64 válido",
            ))
        }
    };
    let signature = match Signature::from_slice(&signature_bytes) {
        Ok(s) => s,
        Err(_) => {
            return Ok(Verification::new(
                SignatureState::InvalidSignature,
                Some(sig.key_id),
                Some(sig.public_key_fingerprint),
                Some(computed),
                "firma con longitud inválida",
            ))
        }
    };
    if verifying.verify(payload.as_bytes(), &signature).is_err() {
        return Ok(Verification::new(
            SignatureState::InvalidSignature,
            Some(sig.key_id),
            Some(sig.public_key_fingerprint),
            Some(computed),
            "firma Ed25519 no válida",
        ));
    }
    let key_revoked = keys::find(home, &sig.key_id)
        .map(|k| k.revoked_at.is_some())
        .unwrap_or(false);
    if trusted.revoked_at.is_some() || key_revoked {
        return Ok(Verification::new(
            SignatureState::SignedRevoked,
            Some(sig.key_id),
            Some(sig.public_key_fingerprint),
            Some(computed),
            "clave revocada",
        ));
    }
    Ok(Verification::new(
        SignatureState::SignedTrusted,
        Some(sig.key_id),
        Some(sig.public_key_fingerprint),
        Some(computed),
        "firma válida y clave confiada",
    ))
}

pub fn verify_package(path: &Path, home: &Path) -> Result<Verification> {
    let files = read_package(path)?;
    verify_files(&files, home)
}

/// Estado de un módulo ya instalado, recalculado desde sus archivos.
pub fn installed_state(dir: &Path, home: &Path) -> Result<String> {
    let mut files = Vec::new();
    collect_files(dir, dir, &mut files)?;
    match verify_files(&files, home) {
        Ok(v) => Ok(v.state.as_str().to_string()),
        Err(_) => Ok(SignatureState::InvalidSignature.as_str().to_string()),
    }
}

fn collect_files(root: &Path, dir: &Path, out: &mut Vec<ArchiveFile>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let ft = entry.file_type()?;
        if ft.is_dir() {
            collect_files(root, &path, out)?;
        } else if ft.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| HliError::internal("ruta fuera del módulo"))?;
            let rel = relative
                .components()
                .map(|c| c.as_os_str().to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join("/");
            out.push(ArchiveFile {
                path: rel,
                bytes: std::fs::read(&path)?,
            });
        }
    }
    Ok(())
}
