//! Nginx configuration provider.
//!
//! Responsibilities (specs §Nginx):
//! - auto-detect layout (sites-available/enabled vs conf.d), with manual override,
//! - render server blocks from `assets/nginx/site.conf.j2`,
//! - expose path computation used by the privileged helper.

use std::path::PathBuf;

use minijinja::Environment;
use serde::Serialize;

use crate::config::{Config, NginxLayout};
use crate::domain::{Site, SslCertificate};
use crate::error::{Error, Result};

const SITE_CONF: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../assets/nginx/site.conf.j2"
));

/// Resolved nginx filesystem locations.
#[derive(Debug, Clone)]
pub struct NginxPaths {
    pub sites_available: PathBuf,
    pub sites_enabled: PathBuf,
    pub conf_d: PathBuf,
}

impl Default for NginxPaths {
    fn default() -> Self {
        Self {
            sites_available: PathBuf::from("/etc/nginx/sites-available"),
            sites_enabled: PathBuf::from("/etc/nginx/sites-enabled"),
            conf_d: PathBuf::from("/etc/nginx/conf.d"),
        }
    }
}

impl NginxPaths {
    /// Build from config overrides (falling back to distro defaults).
    pub fn from_config(cfg: &Config) -> Self {
        Self {
            sites_available: cfg
                .nginx_sites_available
                .clone()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/etc/nginx/sites-available")),
            sites_enabled: cfg
                .nginx_sites_enabled
                .clone()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/etc/nginx/sites-enabled")),
            conf_d: cfg
                .nginx_conf_d
                .clone()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/etc/nginx/conf.d")),
        }
    }

    /// Resolve the effective layout per specs rules:
    /// 1. If config pins a layout (Sites/Confd), honor it.
    /// 2. Auto: prefer sites-enabled/sites-available if available; fall back to conf.d.
    pub fn detect_layout(&self, configured: NginxLayout) -> NginxLayout {
        match configured {
            NginxLayout::Sites => NginxLayout::Sites,
            NginxLayout::Confd => NginxLayout::Confd,
            NginxLayout::Auto => {
                if self.sites_available.exists() || self.sites_enabled.exists() {
                    NginxLayout::Sites
                } else if self.conf_d.exists() {
                    NginxLayout::Confd
                } else {
                    // Nothing present; default to the classic layout.
                    NginxLayout::Sites
                }
            }
        }
    }

    /// Where a site's config file should live, plus its enabled symlink (if any).
    pub fn target_for(&self, layout: NginxLayout, name: &str) -> (PathBuf, Option<PathBuf>) {
        let file = format!("{name}.conf");
        match layout {
            NginxLayout::Sites | NginxLayout::Auto => (
                self.sites_available.join(&file),
                Some(self.sites_enabled.join(&file)),
            ),
            NginxLayout::Confd => (self.conf_d.join(&file), None),
        }
    }
}

/// The server_name list (primary + wildcard + aliases) as a single string.
pub fn server_names(site: &Site) -> String {
    let mut names: Vec<String> = Vec::new();
    names.push(site.primary_domain.trim_start_matches("*.").to_string());
    if site.wildcard {
        names.push(format!("*.{}", names[0]));
    }
    for a in &site.aliases {
        if !names.contains(a) {
            names.push(a.clone());
        }
    }
    names.join(" ")
}

/// Default php-fpm socket path for a version (e.g. 8.3 -> /run/php/php8.3-fpm.sock).
pub fn php_fpm_socket(version: &str) -> String {
    format!("unix:/run/php/php{version}-fpm.sock")
}

/// Render the nginx server block for a site.
pub fn render(site: &Site, cert: Option<&SslCertificate>, layout: NginxLayout) -> Result<String> {
    let ctx = RenderContext {
        name: &site.name,
        layout: layout.as_str(),
        server_names: server_names(site),
        site_type: site.site_type.as_str(),
        root: site.project_path.clone(),
        proxy_target: site.proxy_target.clone().unwrap_or_default(),
        websocket: site.websocket,
        php_fpm_socket: site
            .php_version
            .as_deref()
            .map(php_fpm_socket)
            .unwrap_or_default(),
        ssl: cert.is_some(),
        ssl_cert: cert.map(|c| c.cert_path.clone()).unwrap_or_default(),
        ssl_key: cert.map(|c| c.key_path.clone()).unwrap_or_default(),
    };

    let env = Environment::new();
    let rendered = {
        let tmpl = env
            .template_from_str(SITE_CONF)
            .map_err(|e| Error::Template(e.to_string()))?;
        tmpl.render(&ctx)
            .map_err(|e| Error::Template(e.to_string()))?
    };
    Ok(rendered.trim().to_string())
}

#[derive(Serialize)]
struct RenderContext<'a> {
    name: &'a str,
    layout: &'a str,
    server_names: String,
    site_type: &'a str,
    root: String,
    proxy_target: String,
    websocket: bool,
    php_fpm_socket: String,
    ssl: bool,
    ssl_cert: String,
    ssl_key: String,
}

/// Parse an nginx config test output; returns (ok, message).
pub fn parse_test_output(stdout: &str, stderr: &str, success: bool) -> (bool, String) {
    let combined = if stderr.is_empty() {
        stdout.to_string()
    } else {
        stderr.to_string()
    };
    if success {
        (true, "syntax is ok, config test passed".to_string())
    } else {
        (false, combined.trim().to_string())
    }
}

/// Minimal check that rendered config has balanced braces (cheap sanity guard).
pub fn braces_balanced(s: &str) -> bool {
    let mut depth = 0i32;
    for ch in s.chars() {
        match ch {
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
        if depth < 0 {
            return false;
        }
    }
    depth == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Site, SiteType};

    fn site() -> Site {
        Site {
            id: 1,
            name: "app".into(),
            primary_domain: "app.test".into(),
            aliases: vec!["www.app.test".into()],
            wildcard: true,
            site_type: SiteType::Php,
            project_path: "/var/www/app/public".into(),
            php_version: Some("8.3".into()),
            proxy_target: None,
            websocket: true,
            runtime: None,
            ssl_cert_id: None,
            template: None,
            created_at: "now".into(),
            updated_at: "now".into(),
        }
    }

    #[test]
    fn renders_php_block() {
        let out = render(&site(), None, NginxLayout::Sites).unwrap();
        assert!(out.contains("listen 80;"));
        assert!(out.contains("server_name app.test *.app.test www.app.test"));
        assert!(out.contains("fastcgi_pass unix:/run/php/php8.3-fpm.sock"));
        assert!(out.contains("include fastcgi_params;"));
        assert!(!out.contains("snippets/fastcgi-php.conf"));
        assert!(braces_balanced(&out));
        // No SSL block without a cert.
        assert!(!out.contains("listen 443"));
    }

    #[test]
    fn renders_proxy_block() {
        let mut s = site();
        s.site_type = SiteType::Proxy;
        s.proxy_target = Some("127.0.0.1:3000".into());
        let out = render(&s, None, NginxLayout::Confd).unwrap();
        assert!(out.contains("proxy_pass http://127.0.0.1:3000"));
        assert!(out.contains("proxy_set_header Upgrade"));
        assert!(out.contains("proxy_cache_bypass"));
        assert!(braces_balanced(&out));
    }

    #[test]
    fn renders_ssl_block() {
        let cert = SslCertificate {
            id: 1,
            site_id: Some(1),
            domains: vec!["app.test".into()],
            cert_path: "/c/app.crt".into(),
            key_path: "/c/app.key".into(),
            provider: "internal".into(),
            not_before: "x".into(),
            not_after: "y".into(),
            fingerprint: "f".into(),
            created_at: "z".into(),
        };
        let out = render(&site(), Some(&cert), NginxLayout::Sites).unwrap();
        assert!(out.contains("listen 443 ssl"));
        assert!(out.contains("ssl_certificate     /c/app.crt"));
    }

    #[test]
    fn detect_layout_auto() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = NginxPaths {
            sites_available: tmp.path().join("sites-available"),
            sites_enabled: tmp.path().join("sites-enabled"),
            conf_d: tmp.path().join("conf.d"),
        };
        assert_eq!(paths.detect_layout(NginxLayout::Auto), NginxLayout::Sites); // nothing present -> default Sites
        std::fs::create_dir_all(&paths.conf_d).unwrap();
        // Only conf.d present, classic layout absent -> Confd.
        assert_eq!(paths.detect_layout(NginxLayout::Auto), NginxLayout::Confd);
        std::fs::create_dir_all(&paths.sites_available).unwrap();
        // Classic layout present -> prefer Sites.
        assert_eq!(paths.detect_layout(NginxLayout::Auto), NginxLayout::Sites);
        assert_eq!(paths.detect_layout(NginxLayout::Confd), NginxLayout::Confd);
    }
}
