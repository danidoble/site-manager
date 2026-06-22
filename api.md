# Site Manager CLI

Command name: `site-manager`

Global options:

```sh
site-manager --dry-run <command>
site-manager --help
site-manager --version
```

## Status

```sh
site-manager status
site-manager diagnose
```

## Sites

```sh
site-manager site create \
  --name app \
  --domain app.test \
  --type static \
  --configure
```

Create options:

```sh
--name <name>
--domain <domain>
--aliases <a,b,c>
--wildcard
--type <static|php|proxy>
--path <path>
--php <version>
--proxy <host:port>
--runtime <runtime>
--template <name>
--ssl
--configure
--hosts
--no-websocket
```

Other site commands:

```sh
site-manager site list [--search text] [--page n] [--per-page n]
site-manager site show <id-or-name>
site-manager site configure <id-or-name> [--ssl]
site-manager site open <id-or-name>
site-manager site delete <id-or-name>
```

## SSL

```sh
site-manager ssl list
site-manager ssl create --site <id-or-name>
site-manager ssl create --domains app.test,www.app.test
site-manager ssl renew <cert-id>
site-manager ssl delete <cert-id>
```

## CA

```sh
site-manager ca init
site-manager ca show
site-manager ca install                  # system trust store, uses pkexec
site-manager ca install --browser firefox
site-manager ca install --browser all    # Firefox profiles + Chromium NSS DB
```

## Nginx

```sh
site-manager nginx layout
site-manager nginx test
site-manager nginx reload
```

## Services

Allowed services are `nginx`, `dnsmasq`, `local-site-manager.timer`,
`local-site-manager.service`, and `php*-fpm`.

```sh
site-manager service status nginx
site-manager service reload nginx
site-manager service restart nginx
site-manager service restart php8.4-fpm
site-manager service timer
```

## DNS

```sh
site-manager dns wizard --tld test
site-manager dns guides --tld test
site-manager dns apply --tld test
```

Use `site create --hosts` when you want the app to add only that site and its
aliases to `/etc/hosts`. Use DNS/dnsmasq when you prefer wildcard resolution.

## Backups

```sh
site-manager backup create
site-manager backup list
site-manager backup restore <name>
```

## Templates

```sh
site-manager templates
```

## API Server

```sh
site-manager api --port 5847
```

The REST API is served by the `local-site-manager-api` binary; this command
prints the equivalent command to run it.

## Background Worker

```sh
site-manager background
```

Runs nginx validation, renews certificates expiring within 30 days, and probes
proxy health. The packaged systemd timer is `local-site-manager.timer`.
