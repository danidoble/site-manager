//! Error types.

use thiserror::Error;

/// Top-level error type for all core operations.
#[derive(Debug, Error)]
pub enum Error {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("configuration error: {0}")]
    Config(String),

    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("toml parse error: {0}")]
    TomlDe(#[from] toml::de::Error),

    #[error("toml serialize error: {0}")]
    TomlSer(#[from] toml::ser::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("openssl error: {0}")]
    Openssl(#[from] openssl::error::ErrorStack),

    #[error("template error: {0}")]
    Template(String),

    #[error("nginx error: {0}")]
    Nginx(String),

    #[error("validation error: {0}")]
    Validation(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("privileged operation failed: {0}")]
    Privileged(String),

    #[error("http error: {0}")]
    Http(String),

    #[error("{0}")]
    Other(String),
}

/// Convenience Result alias.
pub type Result<T> = std::result::Result<T, Error>;
