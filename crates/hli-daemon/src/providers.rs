//! Configuración pública de proveedores (`providers.toml`).
//!
//! Las claves nunca se guardan aquí: `secret_handle` apunta a `HLI_HOME/secrets/`.

use std::collections::BTreeMap;
use std::path::Path;

use hli_core::{HliError, Result};
use serde::{Deserialize, Serialize};

fn default_timeout() -> u64 {
    120
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProviderEntry {
    pub kind: String,
    pub base_url: String,
    pub model: String,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
    #[serde(default)]
    pub secret_handle: Option<String>,
}

impl ProviderEntry {
    pub fn validate(&self) -> Result<()> {
        if !matches!(self.kind.as_str(), "sim" | "ollama" | "openai") {
            return Err(HliError::invalid(format!(
                "kind de proveedor no soportado: {}",
                self.kind
            )));
        }
        if self.base_url.trim().is_empty() {
            return Err(HliError::invalid("base_url no puede estar vacia"));
        }
        if self.model.trim().is_empty() {
            return Err(HliError::invalid("model no puede estar vacio"));
        }
        if self.timeout_seconds == 0 || self.timeout_seconds > 3600 {
            return Err(HliError::invalid(
                "timeout_seconds fuera de rango (1..3600)",
            ));
        }
        if !self.base_url.starts_with("http://") && !self.base_url.starts_with("https://") {
            return Err(HliError::invalid("base_url debe ser http(s)"));
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct ProvidersFile {
    #[serde(default = "default_config_version")]
    pub config_version: u32,
    #[serde(default)]
    pub default_provider: Option<String>,
    #[serde(default)]
    pub providers: BTreeMap<String, ProviderEntry>,
}

fn default_config_version() -> u32 {
    hli_core::versions::CONFIG_SCHEMA_VERSION
}

pub fn path(home: &Path) -> std::path::PathBuf {
    home.join("providers.toml")
}

pub fn load(home: &Path) -> ProvidersFile {
    std::fs::read_to_string(path(home))
        .ok()
        .and_then(|t| toml::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save(home: &Path, cfg: &ProvidersFile) -> Result<()> {
    let mut cfg = cfg.clone();
    cfg.config_version = hli_core::versions::CONFIG_SCHEMA_VERSION;
    let text = toml::to_string_pretty(&cfg)
        .map_err(|e| HliError::internal(format!("providers.toml: {e}")))?;
    std::fs::write(path(home), text)?;
    Ok(())
}

/// Rechaza configuraciones de proveedores de una versión futura.
pub fn check_version(home: &Path) -> Result<()> {
    let Ok(text) = std::fs::read_to_string(path(home)) else {
        return Ok(());
    };
    let cfg: ProvidersFile = toml::from_str(&text)
        .map_err(|e| HliError::invalid(format!("providers.toml inválido: {e}")))?;
    if cfg.config_version > hli_core::versions::CONFIG_SCHEMA_VERSION {
        return Err(HliError::conflict(format!(
            "providers.toml con versión futura {} > {}; actualiza HLI",
            cfg.config_version,
            hli_core::versions::CONFIG_SCHEMA_VERSION
        )));
    }
    Ok(())
}
