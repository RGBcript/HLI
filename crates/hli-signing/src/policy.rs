//! Política de aceptación de módulos según su estado de firma.

use std::path::Path;

use hli_core::{HliError, Result};
use serde_json::Value;

use crate::SignatureState;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Policy {
    AllowUnsigned,
    #[default]
    WarnUnsigned,
    RequireSigned,
}

impl Policy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AllowUnsigned => "allow-unsigned",
            Self::WarnUnsigned => "warn-unsigned",
            Self::RequireSigned => "require-signed",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value.trim() {
            "allow-unsigned" => Ok(Self::AllowUnsigned),
            "warn-unsigned" => Ok(Self::WarnUnsigned),
            "require-signed" => Ok(Self::RequireSigned),
            other => Err(HliError::invalid(format!(
                "política de firma desconocida: {other}"
            ))),
        }
    }

    /// `HLI_MODULE_POLICY` tiene prioridad sobre `HLI_HOME/module-policy.json`.
    pub fn load(home: &Path) -> Self {
        if let Ok(raw) = std::env::var("HLI_MODULE_POLICY") {
            if let Ok(p) = Self::parse(&raw) {
                return p;
            }
        }
        std::fs::read_to_string(home.join("module-policy.json"))
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|v| v.get("policy").and_then(|p| p.as_str()).map(str::to_string))
            .and_then(|p| Self::parse(&p).ok())
            .unwrap_or_default()
    }
}

#[derive(Clone, Debug)]
pub struct PolicyOutcome {
    pub policy: Policy,
    pub accepted: bool,
    pub warning: Option<String>,
}

pub fn apply(policy: Policy, state: SignatureState) -> PolicyOutcome {
    let warning = match state {
        SignatureState::Unsigned => {
            if policy == Policy::RequireSigned {
                None
            } else if policy == Policy::WarnUnsigned {
                Some("paquete sin firma (warn-unsigned)".to_string())
            } else {
                None
            }
        }
        SignatureState::SignedUntrusted => {
            Some("firma válida de una clave no confiable".to_string())
        }
        SignatureState::SignedTrusted => None,
        _ => None,
    };
    let accepted = match state {
        SignatureState::InvalidSignature | SignatureState::SignedRevoked => false,
        SignatureState::SignedTrusted => true,
        SignatureState::Unsigned => policy != Policy::RequireSigned,
        SignatureState::SignedUntrusted => policy != Policy::RequireSigned,
    };
    PolicyOutcome {
        policy,
        accepted,
        warning,
    }
}
