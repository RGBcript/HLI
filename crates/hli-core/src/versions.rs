//! Versiones independientes de cada formato de HLI.
//!
//! No existe una única constante global: base de datos, eventos, configuración,
//! `.hliar` y `.hlimod` evolucionan por separado.

pub use crate::model::EVENT_SCHEMA_VERSION;

/// Versión del esquema SQLite (ver `hli_store::migrations`).
pub const DATABASE_SCHEMA_VERSION: i64 = 8;
/// Versión de los archivos de configuración (`providers.toml`, trust store...).
pub const CONFIG_SCHEMA_VERSION: u32 = 1;
/// Versión del contenedor `.hliar`.
pub const HLIAR_FORMAT_VERSION: u32 = 1;
/// Versión del paquete `.hlimod`.
pub const HLIMOD_FORMAT_VERSION: u32 = 1;
