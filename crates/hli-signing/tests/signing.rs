//! Pruebas de firma, integridad, trust store y políticas.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use hli_signing::package::{canonical_payload, sign_files, verify_files, SIGNATURE_FILE};
use hli_signing::{keys, policy, trust, Policy, SignatureState};
use hli_store::export::{container_bytes, sha256_hex, ArchiveFile};
use serde_json::{json, Value};

static SEQ: AtomicUsize = AtomicUsize::new(0);

struct Home(PathBuf);

impl Home {
    fn new(tag: &str) -> Self {
        let n = SEQ.fetch_add(1, Ordering::SeqCst);
        let path =
            std::env::temp_dir().join(format!("hli-signing-{}-{tag}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn entry_bytes() -> Vec<u8> {
    b"module-binary-v1".to_vec()
}

fn manifest(entry: &str) -> Value {
    json!({
        "apiVersion": "hli.dev/v1",
        "kind": "Module",
        "metadata": {"name": "signing-fixture", "version": "0.1.0"},
        "spec": {
            "sdk": ">=0.4 <0.5",
            "runtime": {"kind": "subprocess", "entry": entry},
            "operations": [{"name": "memory.search", "effects": "read"}],
            "migrations": [{"id": "0001", "path": "migrations/0001.sql"}],
            "lifecycle": {"health": "health", "shutdown": "shutdown"}
        }
    })
}

fn base_files() -> Vec<ArchiveFile> {
    let entry = "bin/app.bin";
    let manifest_bytes = serde_json::to_vec(&manifest(entry)).unwrap();
    let bin = entry_bytes();
    let checks = json!({
        "manifest.json": format!("sha256:{}", sha256_hex(&manifest_bytes)),
        entry: format!("sha256:{}", sha256_hex(&bin)),
    });
    vec![
        ArchiveFile {
            path: "manifest.json".into(),
            bytes: manifest_bytes,
        },
        ArchiveFile {
            path: entry.into(),
            bytes: bin,
        },
        ArchiveFile {
            path: "checksums.json".into(),
            bytes: serde_json::to_vec(&checks).unwrap(),
        },
        ArchiveFile {
            path: "schemas/search.json".into(),
            bytes: b"{\"type\":\"object\"}".to_vec(),
        },
    ]
}

fn get(files: &[ArchiveFile], path: &str) -> Vec<u8> {
    files
        .iter()
        .find(|f| f.path == path)
        .unwrap_or_else(|| panic!("falta {path}"))
        .bytes
        .clone()
}

fn put(files: &mut Vec<ArchiveFile>, path: &str, bytes: Vec<u8>) {
    if let Some(f) = files.iter_mut().find(|f| f.path == path) {
        f.bytes = bytes;
    } else {
        files.push(ArchiveFile {
            path: path.into(),
            bytes,
        });
    }
}

/// Recalcula los checksums de las rutas que ya cubre `checksums.json`.
fn refresh_checksums(files: &mut Vec<ArchiveFile>) {
    let checks: Value = serde_json::from_slice(&get(files, "checksums.json")).unwrap();
    let mut out = serde_json::Map::new();
    for path in checks.as_object().unwrap().keys() {
        let bytes = get(files, path);
        out.insert(
            path.clone(),
            json!(format!("sha256:{}", sha256_hex(&bytes))),
        );
    }
    put(
        files,
        "checksums.json",
        serde_json::to_vec(&Value::Object(out)).unwrap(),
    );
}

fn key(home: &Home) -> keys::KeyRecord {
    keys::generate(home.path(), "test-key").unwrap()
}

#[test]
fn signed_module_verifies() {
    let home = Home::new("verify");
    let k = key(&home);
    trust::add(home.path(), &k.public_key, "test-key").unwrap();
    let mut files = base_files();
    sign_files(&mut files, home.path(), &k.key_id).unwrap();

    let v = verify_files(&files, home.path()).unwrap();
    assert_eq!(v.state, SignatureState::SignedTrusted);
    assert_eq!(v.key_id.as_deref(), Some(k.key_id.as_str()));
    assert_eq!(v.fingerprint.as_deref(), Some(k.fingerprint.as_str()));
    assert!(v.payload_hash.is_some());
}

#[test]
fn modified_manifest_breaks_signature() {
    let home = Home::new("manifest");
    let k = key(&home);
    trust::add(home.path(), &k.public_key, "test-key").unwrap();
    let mut files = base_files();
    sign_files(&mut files, home.path(), &k.key_id).unwrap();

    let mut m = manifest("bin/app.bin");
    m["metadata"]["version"] = json!("9.9.9");
    put(&mut files, "manifest.json", serde_json::to_vec(&m).unwrap());
    refresh_checksums(&mut files);

    let v = verify_files(&files, home.path()).unwrap();
    assert_eq!(v.state, SignatureState::InvalidSignature);
}

#[test]
fn modified_binary_breaks_signature() {
    let home = Home::new("binary");
    let k = key(&home);
    trust::add(home.path(), &k.public_key, "test-key").unwrap();
    let mut files = base_files();
    sign_files(&mut files, home.path(), &k.key_id).unwrap();

    put(&mut files, "bin/app.bin", b"module-binary-v2".to_vec());
    refresh_checksums(&mut files);

    let v = verify_files(&files, home.path()).unwrap();
    assert_eq!(v.state, SignatureState::InvalidSignature);
}

#[test]
fn modified_checksum_manifest_breaks_signature() {
    let home = Home::new("checksums");
    let k = key(&home);
    trust::add(home.path(), &k.public_key, "test-key").unwrap();
    let mut files = base_files();
    sign_files(&mut files, home.path(), &k.key_id).unwrap();

    let mut checks: Value = serde_json::from_slice(&get(&files, "checksums.json")).unwrap();
    checks["schemas/search.json"] = json!(format!(
        "sha256:{}",
        sha256_hex(&get(&files, "schemas/search.json"))
    ));
    put(
        &mut files,
        "checksums.json",
        serde_json::to_vec(&checks).unwrap(),
    );

    // Los checksums siguen siendo coherentes; lo que cambia es el payload firmado.
    let v = verify_files(&files, home.path()).unwrap();
    assert_eq!(v.state, SignatureState::InvalidSignature);
}

#[test]
fn wrong_public_key_is_rejected() {
    let home = Home::new("wrongkey");
    let k = key(&home);
    let other = keys::generate(home.path(), "other").unwrap();
    trust::add(home.path(), &k.public_key, "test-key").unwrap();
    let mut files = base_files();
    sign_files(&mut files, home.path(), &k.key_id).unwrap();

    let mut sig: Value = serde_json::from_slice(&get(&files, SIGNATURE_FILE)).unwrap();
    sig["public_key_fingerprint"] = json!(other.fingerprint);
    put(
        &mut files,
        SIGNATURE_FILE,
        serde_json::to_vec(&sig).unwrap(),
    );

    let v = verify_files(&files, home.path()).unwrap();
    assert_eq!(v.state, SignatureState::InvalidSignature);
}

#[test]
fn revoked_key_is_rejected() {
    let home = Home::new("revoked");
    let k = key(&home);
    trust::add(home.path(), &k.public_key, "test-key").unwrap();
    let mut files = base_files();
    sign_files(&mut files, home.path(), &k.key_id).unwrap();
    trust::revoke(home.path(), &k.key_id).unwrap();

    let v = verify_files(&files, home.path()).unwrap();
    assert_eq!(v.state, SignatureState::SignedRevoked);
    let outcome = policy::apply(Policy::AllowUnsigned, v.state);
    assert!(!outcome.accepted);
}

#[test]
fn untrusted_signature_is_reported() {
    let home = Home::new("untrusted");
    let k = key(&home);
    let mut files = base_files();
    sign_files(&mut files, home.path(), &k.key_id).unwrap();

    let v = verify_files(&files, home.path()).unwrap();
    assert_eq!(v.state, SignatureState::SignedUntrusted);
    let outcome = policy::apply(Policy::WarnUnsigned, v.state);
    assert!(outcome.accepted);
    assert!(outcome.warning.is_some());
}

#[test]
fn allow_unsigned_accepts_unsigned() {
    let home = Home::new("allow");
    let files = base_files();
    let v = verify_files(&files, home.path()).unwrap();
    assert_eq!(v.state, SignatureState::Unsigned);
    let outcome = policy::apply(Policy::AllowUnsigned, v.state);
    assert!(outcome.accepted);
    assert!(outcome.warning.is_none());
}

#[test]
fn warn_unsigned_reports_warning() {
    let home = Home::new("warn");
    let files = base_files();
    let v = verify_files(&files, home.path()).unwrap();
    let outcome = policy::apply(Policy::WarnUnsigned, v.state);
    assert!(outcome.accepted);
    assert!(outcome.warning.unwrap().contains("sin firma"));
}

#[test]
fn require_signed_rejects_unsigned() {
    let home = Home::new("require");
    let files = base_files();
    let v = verify_files(&files, home.path()).unwrap();
    let outcome = policy::apply(Policy::RequireSigned, v.state);
    assert!(!outcome.accepted);
}

#[test]
fn signing_is_reproducible_for_same_payload() {
    let home = Home::new("repro");
    let k = key(&home);
    let mut a = base_files();
    let mut b = base_files();
    let first = sign_files(&mut a, home.path(), &k.key_id).unwrap();
    let second = sign_files(&mut b, home.path(), &k.key_id).unwrap();
    assert_eq!(first.payload_hash, second.payload_hash);
    assert_eq!(first.signature, second.signature);
}

#[test]
fn canonical_payload_is_order_independent() {
    let files = base_files();
    let mut reversed = files.clone();
    reversed.reverse();
    assert_eq!(
        canonical_payload(&files).unwrap(),
        canonical_payload(&reversed).unwrap()
    );
    let raw_a = container_bytes(&files);
    let mut other = files.clone();
    other.rotate_left(2);
    let raw_b = container_bytes(&other);
    assert_ne!(raw_a, raw_b);
}
