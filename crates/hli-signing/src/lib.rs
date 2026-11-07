//! Firma opcional Ed25519 de paquetes `.hlimod`.
//!
//! - La clave privada nunca entra en SQLite, eventos, manifests ni logs: vive
//!   en `HLI_HOME/keys/<key_id>.key` con permisos restringidos.
//! - La clave pública, el fingerprint y el estado de confianza se guardan en
//!   `HLI_HOME/keys/index.json` y `HLI_HOME/trust.json`.
//! - El payload firmado es un manifiesto canónico de rutas, tamaños y hashes;
//!   no se firma el contenedor crudo.

pub mod keys;
pub mod package;
pub mod policy;
pub mod trust;
mod util;

pub use package::{verify_files, verify_package, SignatureFile, Verification};
pub use policy::{Policy, PolicyOutcome};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignatureState {
    Unsigned,
    SignedUntrusted,
    SignedTrusted,
    SignedRevoked,
    InvalidSignature,
}

impl SignatureState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unsigned => "unsigned",
            Self::SignedUntrusted => "signed-untrusted",
            Self::SignedTrusted => "signed-trusted",
            Self::SignedRevoked => "signed-revoked",
            Self::InvalidSignature => "invalid-signature",
        }
    }
}

impl std::fmt::Display for SignatureState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
