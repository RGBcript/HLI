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

pub fn safe_rel_path(path: &str) -> bool {
    if path.is_empty() {
        return false;
    }
    if path.starts_with('/') || path.starts_with('\\') {
        return false;
    }
    if path.len() >= 2 && path.as_bytes()[1] == b':' {
        return false;
    }
    if path.contains("//") || path.contains("\\\\") {
        return false;
    }
    for comp in path.split(['/', '\\']) {
        if comp.is_empty() || comp == "." || comp == ".." {
            return false;
        }
    }
    true
}

#[derive(Clone, Copy, Debug)]
pub struct ContainerLimits {
    pub total_bytes: u64,
    pub entry_bytes: u64,
    pub entries: usize,
}

pub const HLIMOD_LIMITS: ContainerLimits = ContainerLimits {
    total_bytes: 256 * 1024 * 1024,
    entry_bytes: 128 * 1024 * 1024,
    entries: 10_000,
};

pub const HLIAR_LIMITS: ContainerLimits = ContainerLimits {
    total_bytes: 2 * 1024 * 1024 * 1024,
    entry_bytes: 128 * 1024 * 1024,
    entries: 10_000,
};

pub fn read_container(bytes: &[u8]) -> Result<Vec<ArchiveFile>> {
    read_container_limited(bytes, HLIAR_LIMITS)
}

pub fn read_container_limited(bytes: &[u8], limits: ContainerLimits) -> Result<Vec<ArchiveFile>> {
    if bytes.len() as u64 > limits.total_bytes {
        return Err(HliError::invalid(format!(
            "contenedor excede el limite total: {} > {}",
            bytes.len(),
            limits.total_bytes
        )));
    }
    if !bytes.starts_with(MAGIC) {
        return Err(HliError::invalid("contenedor hliar: firma incorrecta"));
    }
    let mut i = MAGIC.len();
    let mut files = Vec::new();
    let mut seen = std::collections::HashSet::new();
    while i < bytes.len() {
        if files.len() >= limits.entries {
            return Err(HliError::invalid("contenedor excede el limite de entradas"));
        }
        let nl = bytes[i..]
            .iter()
            .position(|b| *b == b'\n')
            .ok_or_else(|| HliError::invalid("contenedor hliar: cabecera truncada"))?;
        let header = std::str::from_utf8(&bytes[i..i + nl])
            .map_err(|_| HliError::invalid("contenedor hliar: cabecera no UTF-8"))?;
        let mut parts = header.splitn(3, ' ');
        if parts.next() != Some("FILE") {
            return Err(HliError::invalid("contenedor hliar: seccion desconocida"));
        }
        let len: usize = parts
            .next()
            .ok_or_else(|| HliError::invalid("contenedor hliar: longitud ausente"))?
            .parse()
            .map_err(|_| HliError::invalid("contenedor hliar: longitud invalida"))?;
        if len as u64 > limits.entry_bytes {
            return Err(HliError::invalid(format!(
                "entrada excede el limite: {len} > {}",
                limits.entry_bytes
            )));
        }
        let path = parts
            .next()
            .ok_or_else(|| HliError::invalid("contenedor hliar: ruta ausente"))?
            .to_string();
        if !safe_rel_path(&path) {
            return Err(HliError::invalid(format!(
                "ruta insegura en contenedor: {path}"
            )));
        }
        #[cfg(windows)]
        let key = path.to_ascii_lowercase();
        #[cfg(not(windows))]
        let key = path.clone();
        if !seen.insert(key) {
            return Err(HliError::invalid(format!(
                "entrada duplicada en contenedor: {path}"
            )));
        }
        let start = i + nl + 1;
        let end = start + len;
        if end > bytes.len() {
            return Err(HliError::invalid("contenedor hliar: contenido truncado"));
        }
        files.push(ArchiveFile {
            path,
            bytes: bytes[start..end].to_vec(),
        });
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
            ArchiveFile {
                path: "a.txt".into(),
                bytes: b"hola".to_vec(),
            },
            ArchiveFile {
                path: "b/c.json".into(),
                bytes: b"{}".to_vec(),
            },
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

#[test]
fn rejects_ambiguous_dot_component() {
    assert!(!safe_rel_path("a/./b"));
    assert!(!safe_rel_path("a/../b"));
}

#[test]
fn rejects_duplicate_entries() {
    let files = vec![
        ArchiveFile {
            path: "same".into(),
            bytes: b"x".to_vec(),
        },
        ArchiveFile {
            path: "same".into(),
            bytes: b"y".to_vec(),
        },
    ];
    let raw = container_bytes(&files);
    assert!(read_container(&raw).is_err());
}

#[test]
fn rejects_entry_over_limit() {
    let files = vec![ArchiveFile {
        path: "a".into(),
        bytes: vec![0u8; 16],
    }];
    let raw = container_bytes(&files);
    let limits = ContainerLimits {
        total_bytes: 1024,
        entry_bytes: 8,
        entries: 10,
    };
    assert!(read_container_limited(&raw, limits).is_err());
}

#[test]
fn rejects_total_over_limit() {
    let files = vec![ArchiveFile {
        path: "a".into(),
        bytes: vec![0u8; 32],
    }];
    let raw = container_bytes(&files);
    let limits = ContainerLimits {
        total_bytes: 16,
        entry_bytes: 64,
        entries: 10,
    };
    assert!(read_container_limited(&raw, limits).is_err());
}
#[cfg(test)]
mod safety_tests {
    use super::*;

    fn bs() -> char {
        char::from(92u8)
    }

    #[test]
    fn rejects_dot_dot_path() {
        assert!(!safe_rel_path("../evil"));
        assert!(!safe_rel_path("a/../../b"));
        assert!(!safe_rel_path(&format!("..{}evil", bs())));
    }

    #[test]
    fn rejects_absolute_path_outside_workspace() {
        assert!(!safe_rel_path("/etc/passwd"));
    }

    #[test]
    fn rejects_windows_drive_escape() {
        assert!(!safe_rel_path("C:/evil"));
        assert!(!safe_rel_path(&format!("C:{}evil", bs())));
    }

    #[test]
    fn rejects_unc_path_escape() {
        assert!(!safe_rel_path(&format!("{0}{0}server{0}share{0}x", bs())));
    }

    #[test]
    fn rejects_hlimod_traversal() {
        let files = vec![ArchiveFile {
            path: "../evil".into(),
            bytes: b"x".to_vec(),
        }];
        let raw = container_bytes(&files);
        assert!(read_container(&raw).is_err());
    }
}
