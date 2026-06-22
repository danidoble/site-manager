# Local Site Manager v2.0

Native GNOME desktop app for Ubuntu/Debian (Rust + GTK4 + libadwaita) that manages
local development sites: automatic Nginx config, a local CA + SSL, multiple PHP-FPM
versions, reverse-proxy support, wildcard domains, a REST API, and a CLI.

Built to the spec in [`specs.md`](specs.md).

## Status

This repository implements the **foundation + core** of the spec, plus the GUI shell:

| Area | State |
|------|-------|
| Workspace, config, SQLite storage, domain model | ✅ implemented, tested |
| Nginx provider (layout detect, render, write/reload) | ✅ implemented, tested |
| Internal CA (openssl, 30-year root) + leaf signing | ✅ implemented, tested |
| mkcert provider | ✅ implemented (needs `mkcert` on PATH) |
| SSL issue/renew, domain + SAN handling | ✅ implemented, tested |
| DNS wizard (dnsmasq / hosts / wildcards) | ✅ implemented |
| Diagnostics (nginx, dns, ssl, php, ports, trust) | ✅ implemented |
| Health probing (proxy upstreams) | ✅ implemented |
| Backups (tar.gz create/list/restore) | ✅ implemented, tested |
| Project templates registry (13 frameworks) | ✅ implemented, tested |
| Validation (domains, names, paths, proxy targets) | ✅ implemented, tested |
| Privileged root worker + Polkit policy | ✅ implemented, dry-run tested |
| CLI (`local-site-manager`) | ✅ implemented, smoke-tested |
| REST API (`:5847`, axum) | ✅ implemented, smoke-tested |
| GTK4 / libadwaita GUI | ✅ compiles, dashboard+sites+SSL+diag+logs+backups wired |
| systemd service/timer auto-install | ⏳ skeleton units shipped, wiring deferred |
| `.deb` + AppImage packaging | ✅ built & verified (`packaging/`) |
| Flatpak packaging | ⏳ deferred (see Roadmap) |

## Workspace layout

```
site-manager/
  Cargo.toml                    # workspace + shared dependency versions
  crates/
    lsm-core/                   # the engine: config, db, domain, nginx, ca, ssl,
                                #   dns, health, diagnostics, backup, templates,
                                #   providers, privileged client, App facade
    lsm-cli/                    # bin: site-manager
    lsm-api/                    # bin: local-site-manager-api  (REST :5847)
    lsm-gui/                    # bin: local-site-manager-gui  (GTK4 + libadwaita)
    lsm-privileged/             # bin: root worker spawned via pkexec
  assets/
    sql/schema.sql              # SQLite DDL (applied idempotently)
    nginx/site.conf.j2          # minijinja server-block template
    polkit/local.lsm.policy     # Polkit policy for the privileged helper
    systemd/                    # service + timer units (skeleton)
    templates/templates.toml    # per-framework template registry
```

## Prerequisites

```sh
sudo apt install build-essential pkg-config \
  libgtk-4-dev libadwaita-1-dev libsqlite3-dev libssl-dev libclang-dev
# Rust (stable)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

`bindgen` (used by gtk-rs) needs libclang: `export LIBCLANG_PATH=/usr/lib/llvm-18/lib`.

## Build & test

```sh
cargo build --workspace
cargo test  --workspace      # 25 unit tests
```

Binaries land in `target/debug/`:
`site-manager`, `local-site-manager-api`, `local-site-manager-gui`,
`local-site-manager-privileged`.

## Install (packages)

Two installable artifacts are produced by the scripts in [`packaging/`](packaging/):

### `.deb` (Ubuntu/Debian)

```sh
# one-time: install the packager
cargo install cargo-deb
# build the package
./packaging/build-deb.sh
# install system-wide
sudo apt install ./target/debian/local-site-manager_2.0.2-1_amd64.deb
```

The `.deb` ships all four binaries (`/usr/bin/`), the GNOME `.desktop` entry,
hicolor icons (16→512 + SVG), AppStream metainfo, the Polkit policy
(`/usr/share/polkit-1/actions/`), and the systemd service/timer skeletons.
Runtime `Depends`: `libgtk-4-1, libadwaita-1-0, pkexec | policykit-1,
polkitd | policykit-1, openssl, ca-certificates`. `Recommends`: `nginx,
php-fpm, dnsmasq, libnss3-tools`.

After install, launch from the app grid (Local Site Manager) or run
`local-site-manager-gui`. Polkit will prompt for auth when the app performs a
privileged action (nginx reload, CA trust install); the
`local.lsm.policy` action allows it.

### AppImage (portable, no install)

```sh
./packaging/build-appimage.sh      # downloads linuxdeploy + appimagetool on first run
./packaging/dist/local-site-manager-2.0.2-x86_64.AppImage
```

Single portable file; runs anywhere x86-64. Note: an AppImage does **not** install
the Polkit policy or systemd units — privileged operations from an AppImage will
still trigger a pkexec prompt, and the background timer is not auto-enabled.

### Release version bump

Before committing and tagging a release, update all versioned files:

```sh
./packaging/bump-version.sh 2.0.1      # Debian package becomes 2.0.1-1
./packaging/bump-version.sh 2.0.1 2    # Debian package becomes 2.0.1-2
```

Then commit, push, and tag with the matching version, for example `v2.0.1`.
The GitHub Actions release workflow validates that the tag matches `Cargo.toml`.

### Uninstall

```sh
sudo apt remove local-site-manager      # .deb
rm packaging/dist/*.AppImage            # AppImage: just delete the file
```

## CLI

```sh
site-manager ca init                       # generate the internal CA
site-manager ca install                    # install CA into system trust
site-manager ca install --browser all      # install CA into browser NSS stores
site-manager site create \
    --name app --domain app.test --type php --php 8.3 \
    --wildcard --aliases www.app.test --configure
site-manager site list
site-manager ssl create --site app         # issue cert (domain + wildcard + SANs)
site-manager ssl renew 1
site-manager ssl delete 1
site-manager nginx test                    # nginx -t (privileged)
site-manager nginx reload                  # systemctl reload nginx (privileged)
site-manager service restart php8.4-fpm
site-manager dns apply --tld test          # write dnsmasq drop-in (privileged)
site-manager diagnose
site-manager status
site-manager backup create
site-manager --dry-run site create ...     # privileged helper runs in dry-run
```

`--dry-run` is global: it forces the privileged helper to print what it would do
and execute nothing — safe in any environment.

## REST API

```sh
local-site-manager-api            # listens on 127.0.0.1:5847
```

Endpoints (all JSON):

```
GET  /api/health
GET  /api/status
GET  /api/diagnostics
GET  /api/templates
GET  /api/sites                      ?search=&page=1&per_page=50
POST /api/sites                      { NewSite }
GET  /api/sites/:id
DEL  /api/sites/:id
POST /api/sites/:id/configure        { ssl: bool }
POST /api/sites/:id/cert
GET  /api/sites/:id/health
GET  /api/certs
POST /api/certs/:id/renew
POST /api/ssl/create                 { site?, domains[] }
POST /api/ssl/renew                  { id }
GET  /api/nginx/test
POST /api/nginx/reload
GET  /api/backups
POST /api/backups
POST /api/backups/:name/restore
```

## GUI

```sh
local-site-manager-gui
```

Tabs: **Dashboard** (status + diagnostics), **Sites** (list, create, open,
configure), **SSL** (certs, init/install CA), **Diagnostics**, **Logs**,
**Backups**. Heavy work runs on worker threads; results reach the UI over an
mpsc channel polled on the GTK main thread, so the UI never blocks.

## Storage

Default: `$XDG_CONFIG_HOME/local-site-manager` (or `~/.config/local-site-manager`).

```
database.sqlite        # sites, aliases, certs, ca, proxies, health_checks
config.toml            # config (dry_run, api_port, layout, provider, paths)
logs/app.log           # tracing log
backups/               # tar.gz archives
certificates/          # issued leaf certs + keys
ca/                    # rootCA.{crt,key}
nginx-generated/       # rendered configs (local copy)
```

Override the root with `storage_root` in `config.toml`.

## Security model (specs §Security)

- GUI / API / CLI **never run as root**.
- Only `local-site-manager-privileged` runs privileged, via **pkexec** + a
  Polkit policy (`assets/polkit/local.lsm.policy`) that requires admin auth.
- All free-form input is validated (domains, site names, paths, proxy targets)
  against allowlists; shell-outs use `Command` arg vectors — never `sh -c` — to
  prevent injection.
- Every privileged action is logged to stderr with its op name.

Install the Polkit policy + helper for production use:

```sh
sudo install -m 0644 assets/polkit/local.lsm.policy \
    /usr/share/polkit-1/actions/local.lsm.policy
sudo install -m 0755 target/release/local-site-manager-privileged \
    /usr/local/bin/local-site-manager-privileged
```

(Then point `config.privileged_helper` at that path, or rely on `PATH`.)

## Roadmap (deferred from the spec)

- systemd service/timer auto-install + `background` wiring to the shipped units.
- Flatpak packaging (CI).
- Richer GUI editors (full site form, backup restore UI, live log streaming).
- Apache / Caddy / Traefik web-server providers behind the `WebServerProvider` trait.
- Custom certificate providers behind the `CertProvider` trait.

## License

MIT.
