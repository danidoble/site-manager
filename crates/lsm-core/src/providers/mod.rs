//! Provider architecture (specs §Providers).
//!
//! Traits leave room for Apache/Caddy/Traefik web servers, Node/Bun/Deno/Python/Go
//! runtimes, and mkcert/custom certificate providers. First implementations live in
//! [`crate::nginx`], [`crate::ssl`], and [`crate::ca`].

use crate::domain::{Site, SslCertificate};
use crate::error::Result;

/// Web server provider (Nginx today; Apache/Caddy/Traefik later).
pub trait WebServerProvider: Send + Sync {
    fn name(&self) -> &'static str;
    fn write_config(&self, site: &Site, cert: Option<&SslCertificate>) -> Result<()>;
    fn test_config(&self) -> Result<bool>;
    fn reload(&self) -> Result<()>;
    fn remove_config(&self, site: &Site) -> Result<()>;
}

/// Certificate provider (internal CA today; mkcert + custom later).
pub trait CertProvider: Send + Sync {
    fn name(&self) -> &'static str;
    fn issue(&self, domains: &[String]) -> Result<SslCertificate>;
    fn renew(&self, cert: &SslCertificate) -> Result<SslCertificate>;
}

/// Runtime provider for proxy backends.
pub trait RuntimeProvider: Send + Sync {
    fn name(&self) -> &'static str;
    fn detect(&self) -> Result<bool>;
}

/// Names of web server providers the UI/API can advertise.
pub const WEB_SERVERS: &[&str] = &["nginx"]; // apache, caddy, traefik (future)

/// Names of supported runtimes.
pub const RUNTIMES: &[&str] = &["node", "bun", "deno", "python", "go", "http"];

/// Names of certificate providers.
pub const CERT_PROVIDERS: &[&str] = &["internal", "mkcert"];
