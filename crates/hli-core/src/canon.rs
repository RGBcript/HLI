//! Canonicalización JSON (RFC 8785, subconjunto) y hash de cadena.
//!
//! Limitaciones deliberadas de 0.1, registradas en ADR-0003:
//! - Los números se serializan con la representación de `serde_json` (ryu para
//!   flotantes), que coincide con JCS en el rango IEEE-754 normal.
//! - Se rechazan valores no finitos: `serde_json` no los produce.
//! - El orden de claves es por code units UTF-16, como exige JCS.

use serde_json::Value;
use sha2::{Digest, Sha256};

pub fn canonical_json(v: &Value) -> String {
    let mut out = String::new();
    write_value(v, &mut out);
    out
}

fn write_value(v: &Value, out: &mut String) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => out.push_str(&n.to_string()),
        Value::String(s) => write_string(s, out),
        Value::Array(a) => {
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_value(x, out);
            }
            out.push(']');
        }
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
            out.push('{');
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_string(k, out);
                out.push(':');
                write_value(&m[*k], out);
            }
            out.push('}');
        }
    }
}

fn write_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// `sha256:<hex>` sobre la forma canónica.
pub fn hash_canonical(v: &Value) -> String {
    let mut h = Sha256::new();
    h.update(canonical_json(v).as_bytes());
    format!("sha256:{}", hex::encode(h.finalize()))
}

pub fn zero_hash() -> String {
    format!("sha256:{}", "0".repeat(64))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn keys_are_sorted() {
        let v = json!({"b": 1, "a": 2});
        assert_eq!(canonical_json(&v), r#"{"a":2,"b":1}"#);
    }

    #[test]
    fn nested_is_stable() {
        let a = json!({"z": [1, {"y": true, "x": "q"}], "a": null});
        let b = json!({"a": null, "z": [1, {"x": "q", "y": true}]});
        assert_eq!(canonical_json(&a), canonical_json(&b));
    }

    #[test]
    fn escapes_control_chars() {
        let v = json!({"k": "a\tb\nc"});
        assert_eq!(canonical_json(&v), r#"{"k":"a\tb\nc"}"#);
    }

    #[test]
    fn hash_is_prefixed_and_stable() {
        let v = json!({"x": 1});
        assert_eq!(hash_canonical(&v), hash_canonical(&v));
        assert!(hash_canonical(&v).starts_with("sha256:"));
        assert_eq!(hash_canonical(&v).len(), 7 + 64);
    }
}
