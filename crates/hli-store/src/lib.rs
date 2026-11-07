//! Almacén de HLI 0.1: SQLite en WAL, un único actor de escritura y
//! proyecciones materializadas reconstruibles desde el event log.

pub mod export;
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

        let conn = Connection::open(&db_path)?;
        schema::apply(&conn)?;
        drop(conn);

        Ok(Self { home, db_path, artifacts_dir, snapshots_dir })
    }

    /// Arranca el actor de escritura y devuelve también un lector independiente.
    pub fn start(self) -> Result<(Writer, Reader)> {
        let conn = Connection::open(&self.db_path)?;
        schema::apply(&conn)?;
        let writer = writer::spawn(conn, self.artifacts_dir.clone(), self.snapshots_dir.clone());
        let reader = Reader::open(&self.db_path, self.artifacts_dir.clone())?;
        Ok((writer, reader))
    }
}

pub fn db_path(home: &Path) -> PathBuf {
    home.join("hli.db")
}
