//! [`App`] — central facade over config, storage, and all providers.
//!
//! CLI, API, and GUI call these methods; they orchestrate validation, the DB,
//! certificate generation, nginx rendering, and privileged operations.

use std::path::{Path, PathBuf};
use std::process::Command;

use tracing_appender::non_blocking::WorkerGuard;

use crate::backup;
use crate::ca;
use crate::config::{Config, NginxLayout, Paths};
use crate::db::{Db, NewCertRow};
use crate::diagnostics;
use crate::dns;
use crate::domain::{
    BackupEntry, Ca, DiagnosticResult, HealthCheck, NewSite, Site, SiteType, SslCertificate, Status,
};
use crate::error::{Error, Result};
use crate::health;
use crate::nginx::{self, NginxPaths};
use crate::privileged::{self, PrivilegedCommand, PrivilegedResult};
use crate::ssl;
use crate::validate;

/// Top-level application handle.
pub struct App {
    pub config: Config,
    pub paths: Paths,
    pub db: Db,
    _log_guard: Option<WorkerGuard>,
}

impl App {
    /// Initialize: resolve storage root, load config, ensure dirs, open DB, start logging.
    pub fn new() -> Result<Self> {
        let root = Config::default().storage_root_path();
        let mut paths = Paths::at(root);
        let mut config = Config::load(&paths.config)?;

        // Honor a storage_root override from the config file.
        let effective_root = config.storage_root_path();
        if effective_root != paths.root {
            paths = Paths::at(effective_root);
            config = Config::load(&paths.config).unwrap_or_else(|_| Config::default());
        }

        paths.ensure_dirs()?;
        let _log_guard = crate::logs::init(&paths.logs);
        let db = Db::open(&paths.db)?;

        Ok(Self {
            config,
            paths,
            db,
            _log_guard,
        })
    }

    /// Build an [`App`] rooted at an explicit directory (testing / CLI override).
    pub fn with_root(root: PathBuf) -> Result<Self> {
        let paths = Paths::at(root);
        paths.ensure_dirs()?;
        let config = Config::load(&paths.config).unwrap_or_default();
        let _log_guard = crate::logs::init(&paths.logs);
        let db = Db::open(&paths.db)?;
        Ok(Self {
            config,
            paths,
            db,
            _log_guard,
        })
    }

    fn now(&self) -> String {
        chrono::Utc::now().to_rfc3339()
    }

    fn nginx_paths(&self) -> NginxPaths {
        NginxPaths::from_config(&self.config)
    }

    fn layout(&self) -> NginxLayout {
        self.nginx_paths().detect_layout(self.config.nginx_layout)
    }

    fn helper(&self) -> &str {
        &self.config.privileged_helper
    }

    // ---- sites -----------------------------------------------------------

    /// Create a site record (validated, persisted). Does not yet write nginx config.
    pub fn create_site(&self, input: NewSite) -> Result<Site> {
        validate::validate_site_name(&input.name)?;
        validate::validate_domain(&input.primary_domain)?;
        for a in &input.aliases {
            validate::validate_domain(a)?;
        }
        if let Some(t) = &input.proxy_target {
            validate::validate_proxy_target(t)?;
        }
        if input.site_type == SiteType::Php && input.php_version.is_none() {
            return Err(Error::Validation("php sites require --php-version".into()));
        }
        if input.site_type == SiteType::Proxy && input.proxy_target.is_none() {
            return Err(Error::Validation(
                "proxy sites require --proxy-target".into(),
            ));
        }

        let project_path = match &input.project_path {
            Some(p) => {
                validate::validate_path(p, Path::new(&self.config.www_root), false)?;
                p.clone()
            }
            None => default_project_path(&self.config.www_root, &input.primary_domain),
        };

        if self.db.find_site_by_name(&input.name)?.is_some() {
            return Err(Error::Validation(format!("site `{}` already exists", input.name)));
        }

        if matches!(input.site_type, SiteType::Static | SiteType::Php) && input.template.is_none() {
            self.ensure_project_dir(&project_path)?;
        }

        self.db.insert_site(&input, &project_path, &self.now())
    }

    pub fn list_sites(&self, search: Option<&str>, page: usize, per_page: usize) -> Result<Vec<Site>> {
        let per_page = per_page.clamp(1, 500) as i64;
        let offset = (page.saturating_sub(1) * per_page as usize) as i64;
        self.db.list_sites(search, per_page, offset)
    }

    pub fn get_site(&self, id: i64) -> Result<Site> {
        self.db.get_site(id)
    }

    pub fn find_site(&self, name: &str) -> Result<Option<Site>> {
        self.db.find_site_by_name(name)
    }

    pub fn update_site(&self, mut site: Site) -> Result<Site> {
        validate::validate_domain(&site.primary_domain)?;
        validate::validate_path(&site.project_path, Path::new(&self.config.www_root), false)?;
        if let Some(t) = &site.proxy_target {
            validate::validate_proxy_target(t)?;
        }
        if site.site_type == SiteType::Php && site.php_version.is_none() {
            return Err(Error::Validation("php sites require a php version".into()));
        }
        if site.site_type == SiteType::Proxy && site.proxy_target.is_none() {
            return Err(Error::Validation("proxy sites require a proxy target".into()));
        }
        if matches!(site.site_type, SiteType::Static | SiteType::Php) {
            self.ensure_project_dir(&site.project_path)?;
        }
        site.updated_at = self.now();
        self.db.update_site(&site, &site.updated_at)?;
        self.db.get_site(site.id)
    }

    /// Delete a site: remove its nginx config (privileged), then the DB row.
    pub fn delete_site(&self, id: i64) -> Result<()> {
        let site = self.db.get_site(id)?;
        self.remove_site_config(&site)?;
        if let Some(cert_id) = site.ssl_cert_id {
            let _ = self.delete_cert(cert_id);
        }
        let _ = self.remove_hosts_for_site(&site);
        self.db.delete_site(id)
    }

    // ---- nginx -----------------------------------------------------------

    pub fn detect_layout(&self) -> NginxLayout {
        self.layout()
    }

    pub fn nginx_test(&self) -> Result<(bool, String)> {
        let res = privileged::run_capture(
            &PrivilegedCommand::NginxTest,
            self.config.dry_run,
            self.helper(),
        )?;
        let (ok, msg) = nginx::parse_test_output(&res.stdout, &res.stderr, res.success);
        Ok((ok, msg))
    }

    pub fn nginx_reload(&self) -> Result<PrivilegedResult> {
        privileged::run(
            &PrivilegedCommand::NginxReload,
            self.config.dry_run,
            self.helper(),
        )
    }

    pub fn systemctl(&self, action: &str, service: &str) -> Result<PrivilegedResult> {
        privileged::run(
            &PrivilegedCommand::Systemctl {
                action: action.to_string(),
                service: service.to_string(),
            },
            self.config.dry_run,
            self.helper(),
        )
    }

    pub fn systemctl_capture(&self, action: &str, service: &str) -> Result<PrivilegedResult> {
        privileged::run_capture(
            &PrivilegedCommand::Systemctl {
                action: action.to_string(),
                service: service.to_string(),
            },
            self.config.dry_run,
            self.helper(),
        )
    }

    pub fn install_auto_renew_timer(&self) -> Result<PrivilegedResult> {
        privileged::run(
            &PrivilegedCommand::InstallAutoRenewTimer {
                service_content: include_str!("../../../assets/systemd/local-site-manager.service")
                    .to_string(),
                timer_content: include_str!("../../../assets/systemd/local-site-manager.timer")
                    .to_string(),
            },
            self.config.dry_run,
            self.helper(),
        )
    }

    fn ensure_project_dir(&self, path: &str) -> Result<()> {
        match std::fs::create_dir_all(path) {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                privileged::run(
                    &PrivilegedCommand::EnsureDir {
                        path: path.to_string(),
                    },
                    self.config.dry_run,
                    self.helper(),
                )?;
                Ok(())
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Write the rendered nginx config for a site (privileged; dry-run safe).
    pub fn write_site_config(&self, site: &Site) -> Result<()> {
        let cert = match site.ssl_cert_id {
            Some(id) => self.db.get_cert(id).ok(),
            None => None,
        };
        let rendered = nginx::render(site, cert.as_ref(), self.layout())?;
        if !nginx::braces_balanced(&rendered) {
            return Err(Error::Nginx("generated config has unbalanced braces".into()));
        }

        // Always keep a local copy under the storage root.
        let local = self.paths.nginx_out.join(format!("{}.conf", site.name));
        std::fs::write(&local, &rendered)?;

        let (target, symlink) = self
            .nginx_paths()
            .target_for(self.layout(), &site.name);
        privileged::run(
            &PrivilegedCommand::WriteNginxConfig {
                target_path: target.to_string_lossy().to_string(),
                symlink_path: symlink.map(|p| p.to_string_lossy().to_string()),
                content: rendered,
            },
            self.config.dry_run,
            self.helper(),
        )?;
        Ok(())
    }

    pub fn remove_site_config(&self, site: &Site) -> Result<()> {
        let _ = std::fs::remove_file(self.paths.nginx_out.join(format!("{}.conf", site.name)));
        let (target, symlink) = self.nginx_paths().target_for(self.layout(), &site.name);
        privileged::run(
            &PrivilegedCommand::RemoveNginxConfig {
                target_path: target.to_string_lossy().to_string(),
                symlink_path: symlink.map(|p| p.to_string_lossy().to_string()),
            },
            self.config.dry_run,
            self.helper(),
        )?;
        Ok(())
    }

    /// Configure a site end-to-end: optionally issue SSL, write nginx config, then
    /// test + reload. Returns the (possibly new) certificate if issued.
    pub fn configure_site(&self, id: i64, issue_ssl: bool) -> Result<Option<SslCertificate>> {
        let mut site = self.db.get_site(id)?;
        let cert = if issue_ssl && site.ssl_cert_id.is_none() {
            let c = self.issue_site_cert(id)?;
            site.ssl_cert_id = Some(c.id);
            Some(c)
        } else {
            site.ssl_cert_id.and_then(|cid| self.db.get_cert(cid).ok())
        };
        self.write_site_config(&site)?;
        let (ok, msg) = self.nginx_test()?;
        if !ok {
            return Err(Error::Nginx(format!("nginx -t failed: {msg}")));
        }
        self.nginx_reload()?;
        Ok(cert)
    }

    // ---- CA --------------------------------------------------------------

    /// Generate (or regenerate) the internal CA.
    pub fn init_ca(&self) -> Result<Ca> {
        let ca = ca::generate_ca(&self.paths.ca, "internal")?;
        self.db.upsert_ca(&ca, &self.now())?;
        Ok(ca)
    }

    pub fn ca_info(&self) -> Result<Option<Ca>> {
        Ok(self.db.get_ca()?.or_else(|| ca::load_ca(&self.paths.ca, "internal").ok().flatten()))
    }

    /// Install the CA into the system trust store (privileged). `browser` None = system only.
    pub fn install_ca(&self, browser: Option<&str>) -> Result<PrivilegedResult> {
        let ca = self.ca_info()?.ok_or_else(|| Error::NotFound("no CA initialized".into()))?;
        if let Some(b) = browser {
            privileged::run(
                &PrivilegedCommand::InstallCaBrowser {
                    browser: b.to_string(),
                    cert_path: ca.cert_path.clone(),
                },
                self.config.dry_run,
                self.helper(),
            )
        } else {
            privileged::run(
                &PrivilegedCommand::InstallCaSystem {
                    cert_path: ca.cert_path.clone(),
                },
                self.config.dry_run,
                self.helper(),
            )
        }
    }

    // ---- SSL -------------------------------------------------------------

    /// Domains covered by a site's certificate.
    pub fn site_domains(&self, site: &Site) -> Vec<String> {
        let mut out: Vec<String> = vec![site.primary_domain.trim_start_matches("*.").to_string()];
        if site.wildcard {
            out.push(format!("*.{}", out[0]));
        }
        for a in &site.aliases {
            if !out.contains(a) {
                out.push(a.clone());
            }
        }
        out
    }

    /// Issue a certificate for a site using the configured provider.
    pub fn issue_site_cert(&self, site_id: i64) -> Result<SslCertificate> {
        let site = self.db.get_site(site_id)?;
        let old_cert_id = site.ssl_cert_id;
        let domains = self.site_domains(&site);
        let cert = self.issue_domains(Some(site_id), &site.name, &domains)?;
        if let Some(old_id) = old_cert_id {
            if old_id != cert.id {
                let _ = self.delete_cert(old_id);
            }
        }
        Ok(cert)
    }

    /// Issue a standalone certificate for an explicit domain list.
    pub fn issue_domains(&self, site_id: Option<i64>, name: &str, domains: &[String]) -> Result<SslCertificate> {
        if domains.is_empty() {
            return Err(Error::Validation("no domains provided".into()));
        }
        for d in domains {
            validate::validate_domain(d)?;
        }
        let issued = match self.config.cert_provider {
            crate::config::CertProvider::Internal => {
                ssl::issue_internal(&self.paths.ca, &self.paths.certs, name, domains)?
            }
            crate::config::CertProvider::Mkcert => {
                ssl::issue_mkcert(&self.paths.certs, name, domains)?
            }
        };
        let row = NewCertRow {
            site_id,
            provider: issued.provider.clone(),
            domains: issued.domains.clone(),
            cert_path: issued.cert_path.clone(),
            key_path: issued.key_path.clone(),
            not_before: issued.not_before.clone(),
            not_after: issued.not_after.clone(),
            fingerprint: issued.fingerprint.clone(),
        };
        let id = self.db.insert_cert(&row, &self.now())?;
        if let Some(sid) = site_id {
            self.db.set_site_cert(sid, id, &self.now())?;
        }
        self.db.get_cert(id)
    }

    pub fn list_certs(&self) -> Result<Vec<SslCertificate>> {
        self.db.list_certs()
    }

    pub fn delete_cert(&self, id: i64) -> Result<()> {
        let cert = self.db.get_cert(id)?;
        let _ = std::fs::remove_file(&cert.cert_path);
        let _ = std::fs::remove_file(&cert.key_path);
        self.db.detach_cert(id)?;
        self.db.delete_cert(id)
    }

    /// Renew: re-issue the same domains and attach.
    pub fn renew_cert(&self, id: i64) -> Result<SslCertificate> {
        let cert = self.db.get_cert(id)?;
        let name = format!("renewed-{id}");
        self.issue_domains(cert.site_id, &name, &cert.domains)
    }

    pub fn add_hosts_for_site(&self, site: &Site) -> Result<PrivilegedResult> {
        privileged::run(
            &PrivilegedCommand::AddHosts {
                site_name: site.name.clone(),
                domains: self.site_domains(site),
            },
            self.config.dry_run,
            self.helper(),
        )
    }

    pub fn remove_hosts_for_site(&self, site: &Site) -> Result<PrivilegedResult> {
        privileged::run(
            &PrivilegedCommand::RemoveHosts {
                site_name: site.name.clone(),
            },
            self.config.dry_run,
            self.helper(),
        )
    }

    // ---- status / diagnostics / health ----------------------------------

    pub fn status(&self) -> Result<Status> {
        let count = self.db.count_sites()? as usize;
        let php_versions = diagnostics::detect_php_fpm_versions();
        let nginx_status = if which("nginx").is_some() {
            "installed".to_string()
        } else {
            "not installed".to_string()
        };
        let dnsmasq_status = if which("dnsmasq").is_some() {
            "installed".to_string()
        } else {
            "not installed".to_string()
        };
        let ca_present = self.ca_info()?.is_some();
        let ssl_status = if ca_present { "ready".to_string() } else { "no CA".to_string() };
        let php_status = if php_versions.is_empty() {
            "none".to_string()
        } else {
            php_versions.join(", ")
        };
        Ok(Status {
            sites_count: count,
            ssl_status,
            nginx_status,
            dnsmasq_status,
            php_status,
            php_versions,
            nginx_layout: self.layout().as_str().to_string(),
        })
    }

    pub fn diagnose(&self) -> Result<Vec<DiagnosticResult>> {
        let ca_present = self.ca_info()?.is_some();
        Ok(diagnostics::run(&self.config, &self.paths, ca_present))
    }

    /// Probe a proxy site and persist the result.
    pub fn check_proxy(&self, site_id: i64) -> Result<HealthCheck> {
        let site = self.db.get_site(site_id)?;
        let target = site
            .proxy_target
            .ok_or_else(|| Error::Validation("site has no proxy target".into()))?;
        let h = health::probe(site_id, &target);
        self.db.record_health(&h)?;
        Ok(h)
    }

    pub fn latest_health(&self, site_id: i64) -> Result<Option<HealthCheck>> {
        self.db.latest_health(site_id)
    }

    // ---- DNS -------------------------------------------------------------

    /// Render the dnsmasq drop-in for a TLD (localhost).
    pub fn dnsmasq_config(&self, tld: &str) -> String {
        dns::dnsmasq_snippet(tld, "127.0.0.1")
    }

    pub fn dns_guides(&self, tld: &str) -> (String, String, String) {
        (
            dns::guide_dnsmasq(tld, "127.0.0.1"),
            dns::guide_hosts(&format!("app.{tld}"), "127.0.0.1"),
            dns::guide_wildcards(tld),
        )
    }

    /// Apply a dnsmasq drop-in (privileged).
    pub fn apply_dnsmasq(&self, tld: &str) -> Result<PrivilegedResult> {
        let content = self.dnsmasq_config(tld);
        privileged::run(
            &PrivilegedCommand::SetDnsmasq {
                target_path: dns::dnsmasq_target(),
                content,
            },
            self.config.dry_run,
            self.helper(),
        )
    }

    // ---- backup ----------------------------------------------------------

    pub fn backup_create(&self) -> Result<BackupEntry> {
        let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        backup::create_backup(&self.paths.backups, &self.paths.root, &stamp)
    }

    pub fn backup_list(&self) -> Result<Vec<BackupEntry>> {
        backup::list_backups(&self.paths.backups)
    }

    pub fn backup_restore(&self, name: &str) -> Result<Vec<PathBuf>> {
        let entry = self
            .backup_list()?
            .into_iter()
            .find(|b| b.name == name || b.id == name)
            .ok_or_else(|| Error::NotFound(format!("backup {name}")))?;
        let dest = self.paths.root.join("restored");
        backup::restore_backup(Path::new(&entry.path), &dest)
    }

    pub fn backup_delete(&self, name: &str) -> Result<()> {
        backup::delete_backup(&self.paths.backups, name)
    }

    // ---- misc ------------------------------------------------------------

    /// Open a URL in the default browser via xdg-open.
    pub fn open_in_browser(url: &str) -> Result<()> {
        Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|e| Error::Other(format!("xdg-open: {e}")))?;
        Ok(())
    }

    pub fn templates(&self) -> Vec<crate::templates::ProjectTemplate> {
        crate::templates::all()
    }
}

fn which(bin: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(bin);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn default_project_path(www_root: &str, domain: &str) -> String {
    let domain = domain.trim().trim_start_matches("*.");
    format!("{}/{domain}/html", www_root.trim_end_matches('/'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::NewSite;

    fn temp_app() -> App {
        let dir = tempfile::tempdir().unwrap();
        let mut app = App::with_root(dir.path().join("store")).unwrap();
        app.config.www_root = app.paths.root.join("www").to_string_lossy().to_string();
        app
    }

    #[test]
    fn create_and_list_site() {
        let app = temp_app();
        let s = app
            .create_site(NewSite {
                name: "app".into(),
                primary_domain: "app.test".into(),
                aliases: vec!["www.app.test".into()],
                wildcard: true,
                site_type: SiteType::Static,
                project_path: None,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(s.name, "app");
        assert!(s.project_path.ends_with("/app.test/html"));
        assert!(std::path::Path::new(&s.project_path).is_dir());
        assert_eq!(app.list_sites(None, 1, 50).unwrap().len(), 1);
    }

    #[test]
    fn rejects_invalid_input() {
        let app = temp_app();
        let err = app
            .create_site(NewSite {
                name: "Bad Name".into(),
                primary_domain: "app.test".into(),
                ..Default::default()
            })
            .unwrap_err();
        assert!(matches!(err, Error::Validation(_)));

        let err = app
            .create_site(NewSite {
                name: "ok".into(),
                primary_domain: "/etc/passwd".into(),
                ..Default::default()
            })
            .unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn ca_and_cert_flow() {
        let app = temp_app();
        let ca = app.init_ca().unwrap();
        assert_eq!(ca.fingerprint.len(), 64);
        assert!(app.ca_info().unwrap().is_some());

        let s = app
            .create_site(NewSite {
                name: "app".into(),
                primary_domain: "app.test".into(),
                wildcard: true,
                ..Default::default()
            })
            .unwrap();
        let cert = app.issue_site_cert(s.id).unwrap();
        assert!(cert.domains.contains(&"*.app.test".to_string()));
    }

    #[test]
    fn domains_dedup() {
        let app = temp_app();
        let s = app
            .create_site(NewSite {
                name: "app".into(),
                primary_domain: "app.test".into(),
                aliases: vec!["app.test".into(), "api.app.test".into()],
                wildcard: true,
                ..Default::default()
            })
            .unwrap();
        let d = app.site_domains(&s);
        assert_eq!(d.first().map(|s| s.as_str()), Some("app.test"));
        assert!(d.contains(&"*.app.test".to_string()));
    }
}
