//! SQLite storage layer.
//!
//! Uses `rusqlite` (bundled). Schema is embedded from `assets/sql/schema.sql`
//! and applied idempotently on every open.

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::{Ca, HealthCheck, NewSite, Proxy, Site, SiteType, SslCertificate};
use crate::error::{Error, Result};

const SCHEMA_SQL: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../assets/sql/schema.sql"
));

/// Open SQLite database handle.
pub struct Db {
    pub conn: Connection,
}

impl Db {
    /// Open (creating if needed) and migrate.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             PRAGMA synchronous = NORMAL;",
        )?;
        conn.execute_batch(SCHEMA_SQL)?;
        let _ = conn.execute(
            "ALTER TABLE sites ADD COLUMN websocket INTEGER NOT NULL DEFAULT 1",
            [],
        );
        Ok(Self { conn })
    }

    // ---- sites ------------------------------------------------------------

    /// Insert a site (and its aliases), returning the stored row.
    pub fn insert_site(&self, new: &NewSite, project_path: &str, now: &str) -> Result<Site> {
        let site_type = new.site_type;
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO sites
                (name, primary_domain, wildcard, site_type, project_path,
                 php_version, proxy_target, websocket, runtime, ssl_cert_id, template,
                 created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, ?10, ?11, ?11)",
            params![
                new.name,
                new.primary_domain,
                new.wildcard as i64,
                site_type.as_str(),
                project_path,
                new.php_version.as_deref(),
                new.proxy_target.as_deref(),
                new.websocket as i64,
                new.runtime.as_deref(),
                new.template.as_deref(),
                now,
            ],
        )?;
        let id = tx.last_insert_rowid();
        for alias in &new.aliases {
            tx.execute(
                "INSERT OR IGNORE INTO site_aliases (site_id, domain) VALUES (?1, ?2)",
                params![id, alias],
            )?;
        }
        if let Some(target) = &new.proxy_target {
            let runtime = new.runtime.clone().unwrap_or_else(|| "http".to_string());
            tx.execute(
                "INSERT INTO proxies (site_id, target, runtime) VALUES (?1, ?2, ?3)",
                params![id, target, runtime],
            )?;
        }
        tx.commit()?;
        self.get_site(id)
    }

    /// Attach a freshly-issued certificate to a site.
    pub fn set_site_cert(&self, site_id: i64, cert_id: i64, now: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE sites SET ssl_cert_id = ?1, updated_at = ?2 WHERE id = ?3",
            params![cert_id, now, site_id],
        )?;
        Ok(())
    }

    pub fn list_sites(&self, search: Option<&str>, limit: i64, offset: i64) -> Result<Vec<Site>> {
        let like = format!("%{}%", search.unwrap_or(""));
        let mut stmt = self.conn.prepare(
            "SELECT id FROM sites
             WHERE ?1 = '' OR name LIKE ?2 OR primary_domain LIKE ?2
             ORDER BY name ASC LIMIT ?3 OFFSET ?4",
        )?;
        let ids: Vec<i64> = stmt
            .query_map(params![search.unwrap_or(""), like, limit, offset], |r| {
                r.get(0)
            })?
            .filter_map(|r| r.ok())
            .collect();
        drop(stmt);
        ids.into_iter().map(|id| self.get_site(id)).collect()
    }

    pub fn count_sites(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM sites", [], |r| r.get(0))?)
    }

    pub fn get_site(&self, id: i64) -> Result<Site> {
        let site = self
            .conn
            .query_row(
                "SELECT id, name, primary_domain, wildcard, site_type, project_path,
                        php_version, proxy_target, websocket, runtime, ssl_cert_id, template,
                        created_at, updated_at
                 FROM sites WHERE id = ?1",
                params![id],
                site_from_row,
            )
            .optional()?
            .ok_or_else(|| Error::NotFound(format!("site {id}")))?;
        let aliases = self.site_aliases(id)?;
        let mut s = site;
        s.aliases = aliases;
        Ok(s)
    }

    pub fn find_site_by_name(&self, name: &str) -> Result<Option<Site>> {
        let id: Option<i64> = self
            .conn
            .query_row("SELECT id FROM sites WHERE name = ?1", params![name], |r| {
                r.get(0)
            })
            .optional()?;
        match id {
            Some(id) => Ok(Some(self.get_site(id)?)),
            None => Ok(None),
        }
    }

    pub fn delete_site(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM sites WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn update_site(&self, site: &Site, now: &str) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE sites
                 SET primary_domain = ?1, wildcard = ?2, site_type = ?3, project_path = ?4,
                 php_version = ?5, proxy_target = ?6, websocket = ?7, runtime = ?8, updated_at = ?9
             WHERE id = ?10",
            params![
                site.primary_domain,
                site.wildcard as i64,
                site.site_type.as_str(),
                site.project_path,
                site.php_version.as_deref(),
                site.proxy_target.as_deref(),
                site.websocket as i64,
                site.runtime.as_deref(),
                now,
                site.id,
            ],
        )?;
        tx.execute("DELETE FROM proxies WHERE site_id = ?1", params![site.id])?;
        if let Some(target) = &site.proxy_target {
            let runtime = site.runtime.clone().unwrap_or_else(|| "http".to_string());
            tx.execute(
                "INSERT INTO proxies (site_id, target, runtime) VALUES (?1, ?2, ?3)",
                params![site.id, target, runtime],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    fn site_aliases(&self, site_id: i64) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT domain FROM site_aliases WHERE site_id = ?1 ORDER BY domain")?;
        let rows = stmt.query_map(params![site_id], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn proxies_for_site(&self, site_id: i64) -> Result<Vec<Proxy>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, site_id, target, runtime FROM proxies WHERE site_id = ?1")?;
        let rows = stmt.query_map(params![site_id], |r| {
            Ok(Proxy {
                id: r.get(0)?,
                site_id: r.get(1)?,
                target: r.get(2)?,
                runtime: r.get(3)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    // ---- certificates -----------------------------------------------------

    pub fn insert_cert(&self, c: &NewCertRow, now: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO certificates
                (site_id, provider, domains, cert_path, key_path,
                 not_before, not_after, fingerprint, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                c.site_id,
                c.provider,
                serde_json::to_string(&c.domains)?,
                c.cert_path,
                c.key_path,
                c.not_before,
                c.not_after,
                c.fingerprint,
                now,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn list_certs(&self) -> Result<Vec<SslCertificate>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, site_id, provider, domains, cert_path, key_path,
                    not_before, not_after, fingerprint, created_at
             FROM certificates ORDER BY id DESC",
        )?;
        let rows = stmt.query_map([], cert_from_row)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn get_cert(&self, id: i64) -> Result<SslCertificate> {
        self.conn
            .query_row(
                "SELECT id, site_id, provider, domains, cert_path, key_path,
                        not_before, not_after, fingerprint, created_at
                 FROM certificates WHERE id = ?1",
                params![id],
                cert_from_row,
            )
            .optional()?
            .ok_or_else(|| Error::NotFound(format!("certificate {id}")))
    }

    pub fn delete_cert(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM certificates WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn detach_cert(&self, id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE sites SET ssl_cert_id = NULL WHERE ssl_cert_id = ?1",
            params![id],
        )?;
        Ok(())
    }

    // ---- CA ---------------------------------------------------------------

    pub fn upsert_ca(&self, c: &Ca, now: &str) -> Result<()> {
        self.conn.execute("DELETE FROM ca", [])?;
        self.conn.execute(
            "INSERT INTO ca (provider, name, cert_path, key_path, fingerprint, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                c.provider,
                c.name,
                c.cert_path,
                c.key_path,
                c.fingerprint,
                now
            ],
        )?;
        Ok(())
    }

    pub fn get_ca(&self) -> Result<Option<Ca>> {
        self.conn
            .query_row(
                "SELECT id, provider, name, cert_path, key_path, fingerprint, created_at
                 FROM ca LIMIT 1",
                [],
                |r| {
                    Ok(Ca {
                        id: r.get(0)?,
                        provider: r.get(1)?,
                        name: r.get(2)?,
                        cert_path: r.get(3)?,
                        key_path: r.get(4)?,
                        fingerprint: r.get(5)?,
                        created_at: r.get(6)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    // ---- health -----------------------------------------------------------

    pub fn record_health(&self, h: &HealthCheck) -> Result<()> {
        self.conn.execute(
            "INSERT INTO health_checks (site_id, status_code, healthy, response_ms, checked_at, error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                h.site_id,
                h.status_code.map(|s| s as i64),
                h.healthy as i64,
                h.response_ms.map(|s| s as i64),
                h.checked_at,
                h.error.as_deref(),
            ],
        )?;
        Ok(())
    }

    pub fn latest_health(&self, site_id: i64) -> Result<Option<HealthCheck>> {
        self.conn
            .query_row(
                "SELECT site_id, status_code, healthy, response_ms, checked_at, error
                 FROM health_checks WHERE site_id = ?1 ORDER BY id DESC LIMIT 1",
                params![site_id],
                health_from_row,
            )
            .optional()
            .map_err(Into::into)
    }
}

/// Payload used when persisting a freshly-issued certificate.
pub struct NewCertRow {
    pub site_id: Option<i64>,
    pub provider: String,
    pub domains: Vec<String>,
    pub cert_path: String,
    pub key_path: String,
    pub not_before: String,
    pub not_after: String,
    pub fingerprint: String,
}

fn site_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Site> {
    let site_type_str: String = r.get(4)?;
    Ok(Site {
        id: r.get(0)?,
        name: r.get(1)?,
        primary_domain: r.get(2)?,
        wildcard: r.get::<_, i64>(3)? != 0,
        site_type: SiteType::parse(&site_type_str).unwrap_or(SiteType::Static),
        project_path: r.get(5)?,
        php_version: r.get(6)?,
        proxy_target: r.get(7)?,
        websocket: r.get::<_, i64>(8)? != 0,
        runtime: r.get(9)?,
        ssl_cert_id: r.get(10)?,
        template: r.get(11)?,
        created_at: r.get(12)?,
        updated_at: r.get(13)?,
        aliases: Vec::new(),
    })
}

fn cert_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<SslCertificate> {
    let domains_json: String = r.get(3)?;
    let domains: Vec<String> = serde_json::from_str(&domains_json).unwrap_or_default();
    Ok(SslCertificate {
        id: r.get(0)?,
        site_id: r.get(1)?,
        domains,
        cert_path: r.get(4)?,
        key_path: r.get(5)?,
        not_before: r.get(6)?,
        not_after: r.get(7)?,
        fingerprint: r.get(8)?,
        provider: r.get(2)?,
        created_at: r.get(9)?,
    })
}

fn health_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<HealthCheck> {
    let status_code: Option<i64> = r.get(1)?;
    let response_ms: Option<i64> = r.get(3)?;
    Ok(HealthCheck {
        site_id: r.get(0)?,
        status_code: status_code.map(|s| s as u16),
        healthy: r.get::<_, i64>(2)? != 0,
        response_ms: response_ms.map(|s| s as u64),
        checked_at: r.get(4)?,
        error: r.get(5)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::NewSite;

    fn open_temp() -> Db {
        let dir = tempfile::tempdir().unwrap();
        Db::open(&dir.path().join("test.sqlite")).unwrap()
    }

    #[test]
    fn site_roundtrip() {
        let db = open_temp();
        let now = "2026-01-01T00:00:00Z";
        let new = NewSite {
            name: "app".into(),
            primary_domain: "app.test".into(),
            aliases: vec!["www.app.test".into()],
            wildcard: true,
            site_type: SiteType::Php,
            project_path: Some("/var/www/app".into()),
            php_version: Some("8.3".into()),
            ..Default::default()
        };
        let site = db.insert_site(&new, "/var/www/app", now).unwrap();
        assert_eq!(site.name, "app");
        assert_eq!(site.aliases, vec!["www.app.test".to_string()]);

        let found = db.find_site_by_name("app").unwrap().unwrap();
        assert_eq!(found.id, site.id);

        let listed = db.list_sites(None, 100, 0).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(db.count_sites().unwrap(), 1);

        db.delete_site(site.id).unwrap();
        assert_eq!(db.count_sites().unwrap(), 0);
    }

    #[test]
    fn ca_upsert_replaces() {
        let db = open_temp();
        let now = "2026-01-01T00:00:00Z";
        let ca = Ca {
            id: 0,
            provider: "internal".into(),
            name: "LSM Local CA".into(),
            cert_path: "/x/ca.crt".into(),
            key_path: "/x/ca.key".into(),
            fingerprint: "ab".into(),
            created_at: now.into(),
        };
        db.upsert_ca(&ca, now).unwrap();
        assert!(db.get_ca().unwrap().is_some());
        db.upsert_ca(&ca, now).unwrap();
        // only one CA row
        let count: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM ca", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }
}
