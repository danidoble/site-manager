//! Domain models shared across CLI/API/GUI.

use serde::{Deserialize, Serialize};

/// What kind of site this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SiteType {
    /// Static files / generic web root.
    Static,
    /// PHP-FPM backend.
    Php,
    /// Reverse proxy to an upstream HTTP service.
    Proxy,
}

impl Default for SiteType {
    fn default() -> Self {
        Self::Static
    }
}

impl SiteType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Static => "static",
            Self::Php => "php",
            Self::Proxy => "proxy",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "static" => Some(Self::Static),
            "php" => Some(Self::Php),
            "proxy" => Some(Self::Proxy),
            _ => None,
        }
    }
}

/// Runtime/language for proxy backends (specs §Proxy Support).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Runtime {
    Node,
    Bun,
    Deno,
    Python,
    Go,
    Http,
}

impl Runtime {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "node" | "nodejs" => Some(Self::Node),
            "bun" => Some(Self::Bun),
            "deno" => Some(Self::Deno),
            "python" | "py" => Some(Self::Python),
            "go" | "golang" => Some(Self::Go),
            "http" => Some(Self::Http),
            _ => None,
        }
    }
}

/// A managed site.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Site {
    pub id: i64,
    pub name: String,
    pub primary_domain: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub wildcard: bool,
    #[serde(default)]
    pub site_type: SiteType,
    pub project_path: String,
    #[serde(default)]
    pub php_version: Option<String>,
    #[serde(default)]
    pub proxy_target: Option<String>,
    #[serde(default = "default_websocket")]
    pub websocket: bool,
    #[serde(default)]
    pub runtime: Option<String>,
    #[serde(default)]
    pub ssl_cert_id: Option<i64>,
    #[serde(default)]
    pub template: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Input for creating a site.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NewSite {
    pub name: String,
    pub primary_domain: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub wildcard: bool,
    #[serde(default = "default_site_type")]
    pub site_type: SiteType,
    #[serde(default)]
    pub project_path: Option<String>,
    #[serde(default)]
    pub php_version: Option<String>,
    #[serde(default)]
    pub proxy_target: Option<String>,
    #[serde(default = "default_websocket")]
    pub websocket: bool,
    #[serde(default)]
    pub runtime: Option<String>,
    #[serde(default)]
    pub template: Option<String>,
}

fn default_site_type() -> SiteType {
    SiteType::Static
}

fn default_websocket() -> bool {
    true
}

/// Internal / mkcert certificate authority material.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ca {
    pub id: i64,
    pub provider: String,
    pub name: String,
    pub cert_path: String,
    pub key_path: String,
    pub fingerprint: String,
    pub created_at: String,
}

/// An issued SSL certificate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SslCertificate {
    pub id: i64,
    pub site_id: Option<i64>,
    pub domains: Vec<String>,
    pub cert_path: String,
    pub key_path: String,
    pub provider: String,
    pub not_before: String,
    pub not_after: String,
    pub fingerprint: String,
    pub created_at: String,
}

/// Reverse-proxy mapping.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proxy {
    pub id: i64,
    pub site_id: i64,
    pub target: String,
    pub runtime: String,
}

/// Health probe result for a proxy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    pub site_id: i64,
    pub status_code: Option<u16>,
    pub healthy: bool,
    pub response_ms: Option<u64>,
    pub checked_at: String,
    pub error: Option<String>,
}

/// Detected PHP version on the system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhpVersion {
    pub version: String,
    pub fpm_socket: Option<String>,
}

/// Single diagnostic outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticResult {
    pub name: String,
    pub status: DiagnosticStatus,
    pub message: String,
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticStatus {
    Pass,
    Warn,
    Fail,
}

impl DiagnosticStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Warn => "warn",
            Self::Fail => "fail",
        }
    }
}

/// Dashboard status snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Status {
    pub sites_count: usize,
    pub ssl_status: String,
    pub nginx_status: String,
    pub dnsmasq_status: String,
    pub php_status: String,
    pub php_versions: Vec<String>,
    pub nginx_layout: String,
}

/// A backup archive entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupEntry {
    pub id: String,
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
    pub created_at: String,
}
