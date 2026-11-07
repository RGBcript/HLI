//! Almacén de HLI 0.1: SQLite en WAL, un único actor de escritura y
//! proyecciones materializadas reconstruibles desde el event log.

pub mod export;
pub mod fixtures;
pub mod migrations;
pub mod reader;
pub mod schema;
pub mod writer;

use std::path::{Path, PathBuf};

use hli_core::Result;
use rusqlite::Connection;

pub use reader::Reader;
pub use writer::{WriteCmd, Writer};

pub struct Store {
    pub home: PathBuf,
    pub db_path: PathBuf,
    pub artifacts_dir: PathBuf,
    pub snapshots_dir: PathBuf,
}

impl Store {
    pub fn open(home: impl AsRef<Path>) -> Result<Self> {
        let home = home.as_ref().to_path_buf();
        std::fs::create_dir_all(&home)?;
        let artifacts_dir = home.join("artifacts");
        let snapshots_dir = home.join("snapshots");
        std::fs::create_dir_all(&artifacts_dir)?;
        std::fs::create_dir_all(&snapshots_dir)?;
        let db_path = home.join("hli.db");

        let mut conn = Connection::open(&db_path)?;
        migrations::ensure(&mut conn)?;
        migrations::verify_schema(&conn)?;
        let _ = conn.execute(
            "UPDATE runs SET state = 'interrupted', ended_at = ?1              WHERE state IN ('queued','running','cancelling','waiting_approval')",
            rusqlite::params![hli_core::time::now_iso()],
        );
        let _ = conn.execute(
            "UPDATE mch_instances SET state = 'stopped', pid = NULL              WHERE state IN ('starting','ready','degraded','restarting','stopping')",
            [],
        );
        drop(conn);

        Ok(Self {
            home,
            db_path,
            artifacts_dir,
            snapshots_dir,
        })
    }

    /// Arranca el actor de escritura y devuelve también un lector independiente.
    pub fn start(self) -> Result<(Writer, Reader)> {
        let mut conn = Connection::open(&self.db_path)?;
        migrations::ensure(&mut conn)?;
        migrations::verify_schema(&conn)?;
        let writer = writer::spawn(conn, self.artifacts_dir.clone(), self.snapshots_dir.clone());
        let reader = Reader::open(&self.db_path, self.artifacts_dir.clone())?;
        Ok((writer, reader))
    }
}

pub fn db_path(home: &Path) -> PathBuf {
    home.join("hli.db")
}
