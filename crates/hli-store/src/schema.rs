//! Compatibilidad de esquema: utilidades usadas por el writer.
//!
//! La creación y evolución del esquema vive en `migrations.rs`. Este módulo ya
//! no altera tablas de forma oportunista.

use hli_core::{HliError, Result};
use rusqlite::Connection;

pub fn require_workspace(conn: &Connection, id: &str) -> Result<()> {
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM workspaces WHERE id = ?1", [id], |r| {
        r.get(0)
    })?;
    if n == 0 {
        return Err(HliError::not_found(&format!("workspace {id}")));
    }
    Ok(())
}
