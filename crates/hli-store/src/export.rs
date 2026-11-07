//! Contenedor de exportación `hliar` (provisional, ADR-0004).
//!
//! Formato: `HLIAR1\n` seguido de secciones `FILE <len> <path>\n<bytes>\n`.
//! Determinista: el orden de archivos lo fija el exportador y el manifiesto
//! excluye `exported_at` de su hash.

use hli_core::{canon::hash_canonical, HliError, Result};
use serde_json::Value;

pub const MAGIC: &[u8] = b"HLIAR1\n";
pub const FORMAT: &str = "hli-archive/v1";

#[derive(Clone, Debug)]
pub struct ArchiveFile {
    pub path: String,
    pub bytes: Vec<u8>,
}

pub fn container_bytes(files: &[ArchiveFile]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    for f in files {
        out.extend_from_slice(format!("FILE {} {}\n", f.bytes.len(), f.path).as_bytes());
        out.extend_from_slice(&f.bytes);
        out.push(b'\n');
    }
    out
}

pub fn read_container(bytes: &[u8]) -> Result<Vec<ArchiveFile>> {
    if !bytes.starts_with(MAGIC) {
        return Err(HliError::invalid("contenedor hliar: firma incorrecta"));
    }
    let mut i = MAGIC.len();
    let mut files = Vec::new();
    while i < bytes.len() {
        let nl = bytes[i..]
            .iter()
            .position(|b| *b == b'\n')
            .ok_or_else(|| HliError::invalid("contenedor hliar: cabecera truncada"))?;
        let header = std::str::from_utf8(&bytes[i..i + nl])
            .map_err(|_| HliError::invalid("contenedor hliar: cabecera no UTF-8"))?;
        let mut parts = header.splitn(3, ' ');
        if parts.next() != Some("FILE") {
            return Err(HliError::invalid("contenedor hliar: sección desconocida"));
        }
        let len: usize = parts
            .next()
            .ok_or_else(|| HliError::invalid("contenedor hliar: longitud ausente"))?
            .parse()
            .map_err(|_| HliError::invalid("contenedor hliar: longitud inválida"))?;
        let path = parts
            .next()
            .ok_or_else(|| HliError::invalid("contenedor hliar: ruta ausente"))?
            .to_string();
        let start = i + nl + 1;
        let end = start + len;
        if end > bytes.len() {
            return Err(HliError::invalid("contenedor hliar: contenido truncado"));
        }
        files.push(ArchiveFile { path, bytes: bytes[start..end].to_vec() });
        i = end;
        if i < bytes.len() && bytes[i] == b'\n' {
            i += 1;
        }
    }
    Ok(files)
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

pub fn manifest_hash(manifest: &Value) -> String {
    let mut m = manifest.clone();
    if let Some(o) = m.as_object_mut() {
        o.remove("exported_at");
    }
    hash_canonical(&m)
}

pub fn guess_mime(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "json" => "application/json",
        "md" | "txt" | "log" => "text/plain",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "pdf" => "application/pdf",
        "ndjson" | "jsonl" => "application/x-ndjson",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_container() {
        let files = vec![
            ArchiveFile { path: "a.txt".into(), bytes: b"hola".to_vec() },
            ArchiveFile { path: "b/c.json".into(), bytes: b"{}".to_vec() },
        ];
        let raw = container_bytes(&files);
        let back = read_container(&raw).unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(back[1].path, "b/c.json");
        assert_eq!(back[1].bytes, b"{}");
    }

    #[test]
    fn rejects_bad_magic() {
        assert!(read_container(b"nope").is_err());
    }
}
