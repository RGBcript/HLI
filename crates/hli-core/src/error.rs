use std::fmt;

#[derive(Debug, Clone)]
pub struct HliError {
    pub code: String,
    pub message: String,
}

impl HliError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self { code: code.to_string(), message: message.into() }
    }

    pub fn not_found(what: &str) -> Self {
        Self::new("not_found", format!("{what} no existe"))
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new("invalid", message)
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new("conflict", message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new("internal", message)
    }
}

impl fmt::Display for HliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for HliError {}

impl From<serde_json::Error> for HliError {
    fn from(e: serde_json::Error) -> Self {
        HliError::invalid(format!("json: {e}"))
    }
}

impl From<std::io::Error> for HliError {
    fn from(e: std::io::Error) -> Self {
        HliError::internal(format!("io: {e}"))
    }
}

pub type Result<T> = std::result::Result<T, HliError>;

#[cfg(feature = "sqlite")]
impl From<rusqlite::Error> for HliError {
    fn from(e: rusqlite::Error) -> Self {
        HliError::internal(format!("sqlite: {e}"))
    }
}
