use std::path::PathBuf;
use thiserror::Error;

pub type Result<T, E = MnemosError> = std::result::Result<T, E>;

#[derive(Error, Debug)]
pub enum MnemosError {
    #[error("memory not found: {0}")]
    MemoryNotFound(String),

    #[error("entity not found: {0}")]
    EntityNotFound(String),

    #[error("session not found: {0}")]
    SessionNotFound(String),

    #[error("invalid frontmatter at {path}: {reason}")]
    InvalidFrontmatter { path: PathBuf, reason: String },

    #[error("malformed memory file at {path}: {reason}")]
    MalformedFile { path: PathBuf, reason: String },

    #[error("path resolution failed: {0}")]
    PathError(String),

    #[error("database error: {0}")]
    Database(#[from] libsql::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("yaml error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("migration {version} failed: {reason}")]
    Migration { version: u32, reason: String },

    #[error("schema drift detected: {0}")]
    SchemaDrift(String),

    #[error("validation failed: {0}")]
    Validation(String),

    #[error("internal: {0}")]
    Internal(String),
}
