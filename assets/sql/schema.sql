-- Local Site Manager schema v1
-- Idempotent: safe to run on every open.

CREATE TABLE IF NOT EXISTS sites (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    name            TEXT NOT NULL UNIQUE,
    primary_domain  TEXT NOT NULL,
    wildcard        INTEGER NOT NULL DEFAULT 0,
    site_type       TEXT NOT NULL DEFAULT 'static',
    project_path    TEXT NOT NULL,
    php_version     TEXT,
    proxy_target    TEXT,
    websocket       INTEGER NOT NULL DEFAULT 1,
    runtime         TEXT,
    ssl_cert_id     INTEGER,
    template        TEXT,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_sites_domain ON sites(primary_domain);

CREATE TABLE IF NOT EXISTS site_aliases (
    id      INTEGER PRIMARY KEY AUTOINCREMENT,
    site_id INTEGER NOT NULL REFERENCES sites(id) ON DELETE CASCADE,
    domain  TEXT NOT NULL UNIQUE
);

CREATE INDEX IF NOT EXISTS idx_aliases_site ON site_aliases(site_id);

CREATE TABLE IF NOT EXISTS certificates (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    site_id     INTEGER REFERENCES sites(id) ON DELETE SET NULL,
    provider    TEXT NOT NULL,
    domains     TEXT NOT NULL,           -- JSON array
    cert_path   TEXT NOT NULL,
    key_path    TEXT NOT NULL,
    not_before  TEXT NOT NULL,
    not_after   TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    created_at  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_certs_site ON certificates(site_id);

CREATE TABLE IF NOT EXISTS ca (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    provider    TEXT NOT NULL,
    name        TEXT NOT NULL,
    cert_path   TEXT NOT NULL,
    key_path    TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    created_at  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS proxies (
    id      INTEGER PRIMARY KEY AUTOINCREMENT,
    site_id INTEGER NOT NULL REFERENCES sites(id) ON DELETE CASCADE,
    target  TEXT NOT NULL,
    runtime TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS health_checks (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    site_id     INTEGER NOT NULL REFERENCES sites(id) ON DELETE CASCADE,
    status_code INTEGER,
    healthy     INTEGER NOT NULL DEFAULT 0,
    response_ms INTEGER,
    checked_at  TEXT NOT NULL,
    error       TEXT
);

CREATE INDEX IF NOT EXISTS idx_health_site ON health_checks(site_id);
