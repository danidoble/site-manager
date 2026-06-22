//! Configuration and storage paths.
//!
//! Config lives at `<storage_root>/config.toml`. Default storage root is
//! `$XDG_CONFIG_HOME/local-site-manager` or `~/.config/local-site-manager`.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::Result;

/// Default REST API port (specs).
pub const DEFAULT_API_PORT: u16 = 5847;

/// All on-disk locations managed by the app.
#[derive(Debug, Clone)]
pub struct Paths {
    pub root: PathBuf,
    pub db: PathBuf,
    pub config: PathBuf,
    pub logs: PathBuf,
    pub backups: PathBuf,
    pub certs: PathBuf,
    pub ca: PathBuf,
    pub templates: PathBuf,
    pub nginx_out: PathBuf,
}

impl Paths {
    /// Build a `Paths` rooted at `root`.
    pub fn at(root: PathBuf) -> Self {
        Self {
            db: root.join("database.sqlite"),
            config: root.join("config.toml"),
            logs: root.join("logs"),
            backups: root.join("backups"),
            certs: root.join("certificates"),
            ca: root.join("ca"),
            templates: root.join("templates"),
            nginx_out: root.join("nginx-generated"),
            root,
        }
    }

    /// Default storage root from environment.
    pub fn default_root() -> PathBuf {
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            if !xdg.is_empty() {
                return PathBuf::from(xdg).join("local-site-manager");
            }
        }
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(".config").join("local-site-manager")
    }

    /// Create all subdirectories if missing.
    pub fn ensure_dirs(&self) -> Result<()> {
        for d in [&self.root, &self.logs, &self.backups, &self.certs, &self.ca, &self.templates, &self.nginx_out] {
            fs::create_dir_all(d)?;
        }
        Ok(())
    }
}

/// How nginx config is laid out on this system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NginxLayout {
    /// Auto-detect: prefer sites-available/enabled, fall back to conf.d.
    Auto,
    /// `/etc/nginx/sites-available` + `/etc/nginx/sites-enabled`.
    Sites,
    /// `/etc/nginx/conf.d`.
    Confd,
}

impl Default for NginxLayout {
    fn default() -> Self {
        Self::Auto
    }
}

impl NginxLayout {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Sites => "sites",
            Self::Confd => "confd",
        }
    }
}

/// Certificate provider selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CertProvider {
    /// Internal CA baked into the app (openssl).
    Internal,
    /// Delegate to `mkcert`.
    Mkcert,
}

impl Default for CertProvider {
    fn default() -> Self {
        Self::Internal
    }
}

/// Persisted application configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub dry_run: bool,

    #[serde(default = "default_api_port")]
    pub api_port: u16,

    #[serde(default = "default_web_server")]
    pub web_server: String,

    #[serde(default)]
    pub nginx_layout: NginxLayout,

    #[serde(default)]
    pub cert_provider: CertProvider,

    #[serde(default = "default_www_root")]
    pub www_root: String,

    #[serde(default = "default_php_versions")]
    pub php_versions: Vec<String>,

    /// Override storage root. None = `Paths::default_root()`.
    #[serde(default)]
    pub storage_root: Option<String>,

    /// Path to the privileged helper binary.
    #[serde(default = "default_helper")]
    pub privileged_helper: String,

    /// Nginx sites-available directory (auto when empty).
    #[serde(default)]
    pub nginx_sites_available: Option<String>,

    /// Nginx sites-enabled directory (auto when empty).
    #[serde(default)]
    pub nginx_sites_enabled: Option<String>,

    /// Nginx conf.d directory (auto when empty).
    #[serde(default)]
    pub nginx_conf_d: Option<String>,
}

fn default_api_port() -> u16 {
    DEFAULT_API_PORT
}
fn default_web_server() -> String {
    "nginx".to_string()
}
fn default_www_root() -> String {
    "/var/www".to_string()
}
fn default_php_versions() -> Vec<String> {
    vec![
        "8.0".to_string(),
        "8.1".to_string(),
        "8.2".to_string(),
        "8.3".to_string(),
        "8.4".to_string(),
        "8.5".to_string(),
    ]
}
fn default_helper() -> String {
    "local-site-manager-privileged".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            dry_run: false,
            api_port: default_api_port(),
            web_server: default_web_server(),
            nginx_layout: NginxLayout::default(),
            cert_provider: CertProvider::default(),
            www_root: default_www_root(),
            php_versions: default_php_versions(),
            storage_root: None,
            privileged_helper: default_helper(),
            nginx_sites_available: None,
            nginx_sites_enabled: None,
            nginx_conf_d: None,
        }
    }
}

impl Config {
    /// Resolve the effective storage root (override or default).
    pub fn storage_root_path(&self) -> PathBuf {
        match &self.storage_root {
            Some(s) if !s.is_empty() => PathBuf::from(s),
            _ => Paths::default_root(),
        }
    }

    /// Load config from disk, creating a default if missing.
    pub fn load(path: &Path) -> Result<Self> {
        match fs::read_to_string(path) {
            Ok(text) => {
                let cfg: Config = toml::from_str(&text)?;
                Ok(cfg)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// Persist config to disk as TOML.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self)?;
        fs::write(path, text)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_roundtrip() {
        let cfg = Config::default();
        let toml_s = toml::to_string(&cfg).unwrap();
        let back: Config = toml::from_str(&toml_s).unwrap();
        assert_eq!(cfg.api_port, back.api_port);
        assert_eq!(back.cert_provider, CertProvider::Internal);
        assert_eq!(back.nginx_layout, NginxLayout::Auto);
    }

    #[test]
    fn load_missing_is_default() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("config.toml");
        let cfg = Config::load(&p).unwrap();
        assert_eq!(cfg.api_port, DEFAULT_API_PORT);
    }
}
