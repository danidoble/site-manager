//! Local Site Manager — core engine.
//!
//! All domain logic lives here. CLI, REST API, GUI, and the privileged
//! root worker are thin shells over this crate.

pub mod app;
pub mod backup;
pub mod ca;
pub mod config;
pub mod db;
pub mod diagnostics;
pub mod dns;
pub mod domain;
pub mod error;
pub mod health;
pub mod logs;
pub mod nginx;
pub mod privileged;
pub mod providers;
pub mod ssl;
pub mod templates;
pub mod validate;

pub use app::App;
pub use config::{CertProvider, Config, NginxLayout, Paths};
pub use domain::{
    BackupEntry, Ca, DiagnosticResult, DiagnosticStatus, HealthCheck, NewSite, PhpVersion, Proxy,
    Runtime, Site, SiteType, SslCertificate, Status,
};
pub use error::{Error, Result};
pub use privileged::{PrivilegedCommand, PrivilegedResult};

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
