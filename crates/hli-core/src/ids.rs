use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Identificador corto y estable, con prefijo por tipo.
pub fn new_id(prefix: &str) -> String {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let mut h = Sha256::new();
    h.update(t.to_le_bytes());
    h.update(n.to_le_bytes());
    h.update(std::process::id().to_le_bytes());
    let d = h.finalize();
    format!("{}_{}", prefix, hex::encode(&d[..8]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_in_process() {
        let a = new_id("ws");
        let b = new_id("ws");
        assert_ne!(a, b);
        assert!(a.starts_with("ws_"));
        assert_eq!(a.len(), 3 + 16);
    }
}
