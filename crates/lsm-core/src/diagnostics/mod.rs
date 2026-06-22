//! Diagnostics (specs §Diagnostics).

use std::path::PathBuf;
use std::process::Command;

use crate::config::{Config, Paths};
use crate::domain::{DiagnosticResult, DiagnosticStatus};
use crate::privileged::{self, PrivilegedCommand};

/// Run all diagnostic checks.
pub fn run(config: &Config, _paths: &Paths, ca_present: bool) -> Vec<DiagnosticResult> {
    vec![
        check_nginx(),
        check_nginx_test(config),
        check_dns_resolver(),
        check_dnsmasq(),
        check_openssl(),
        check_php_versions(),
        check_ca(ca_present),
        check_ports(),
        check_browser_tools(),
    ]
}

fn pass(name: &str, msg: impl Into<String>) -> DiagnosticResult {
    DiagnosticResult {
        name: name.into(),
        status: DiagnosticStatus::Pass,
        message: msg.into(),
        detail: None,
    }
}

fn warn(name: &str, msg: impl Into<String>) -> DiagnosticResult {
    DiagnosticResult {
        name: name.into(),
        status: DiagnosticStatus::Warn,
        message: msg.into(),
        detail: None,
    }
}

fn fail(name: &str, msg: impl Into<String>) -> DiagnosticResult {
    DiagnosticResult {
        name: name.into(),
        status: DiagnosticStatus::Fail,
        message: msg.into(),
        detail: None,
    }
}

fn check_nginx() -> DiagnosticResult {
    match which("nginx") {
        Some(_) => pass("nginx", "nginx binary found"),
        None => fail("nginx", "nginx not installed: `sudo apt install nginx`"),
    }
}

fn check_nginx_test(config: &Config) -> DiagnosticResult {
    if which("nginx").is_none() {
        return warn("nginx config", "skipped — nginx not installed");
    }
    match privileged::run_capture(
        &PrivilegedCommand::NginxTest,
        config.dry_run,
        &config.privileged_helper,
    ) {
        Ok(r) if r.success => pass("nginx config", "nginx -t passed"),
        Ok(r) => {
            let detail = if r.stderr.trim().is_empty() {
                r.message
            } else {
                r.stderr.trim().to_string()
            };
            DiagnosticResult {
                name: "nginx config".into(),
                status: DiagnosticStatus::Fail,
                message: "nginx -t failed".into(),
                detail: Some(detail),
            }
        }
        Err(e) => warn("nginx config", format!("could not run privileged nginx -t: {e}")),
    }
}

fn check_dns_resolver() -> DiagnosticResult {
    // Resolve a well-known name via the system resolver.
    match (
        Command::new("getent").args(["hosts", "localhost"]).output(),
        Command::new("getent").args(["hosts", "app.test"]).output(),
    ) {
        (Ok(local), Ok(app)) => {
            if local.status.success() {
                if app.status.success() {
                    warn(
                        "dns",
                        "app.test resolves — a dnsmasq/hosts rule may already be active",
                    )
                } else {
                    pass("dns", "system resolver works; dev TLDs need dnsmasq or /etc/hosts")
                }
            } else {
                fail("dns", "getent could not resolve localhost")
            }
        }
        _ => warn("dns", "getent unavailable; cannot verify resolution"),
    }
}

fn check_dnsmasq() -> DiagnosticResult {
    match which("dnsmasq") {
        Some(_) => pass("dnsmasq", "dnsmasq found"),
        None => warn("dnsmasq", "dnsmasq not found (optional; install for wildcard DNS)"),
    }
}

fn check_openssl() -> DiagnosticResult {
    match which("openssl") {
        Some(_) => pass("openssl", "openssl found"),
        None => fail("openssl", "openssl not found: `sudo apt install openssl`"),
    }
}

fn check_php_versions() -> DiagnosticResult {
    let found = detect_php_fpm_versions();
    if found.is_empty() {
        warn("php", "no php-fpm versions detected")
    } else {
        pass("php", format!("php-fpm detected: {}", found.join(", ")))
    }
}

fn check_ca(ca_present: bool) -> DiagnosticResult {
    if ca_present {
        pass("ca", "internal CA initialized")
    } else {
        warn("ca", "no internal CA yet — run `ca init`")
    }
}

fn check_ports() -> DiagnosticResult {
    // Cheap check: can we bind 80? If not, something (likely nginx) holds it.
    let in_use = std::net::TcpListener::bind("127.0.0.1:80").is_err();
    if in_use {
        pass("ports", "port 80 in use (nginx running)")
    } else {
        warn("ports", "port 80 free — nginx not listening")
    }
}

fn check_browser_tools() -> DiagnosticResult {
    let certutil = which("certutil").is_some();
    if certutil {
        pass("browser trust", "libnss3-tools (certutil) available for browser trust")
    } else {
        warn(
            "browser trust",
            "certutil missing: `sudo apt install libnss3-tools`",
        )
    }
}

/// Scan the system for installed php-fpm versions and their sockets.
pub fn detect_php_fpm_versions() -> Vec<String> {
    let mut out = Vec::new();
    // Common socket path: /run/php/php8.x-fpm.sock
    if let Ok(entries) = std::fs::read_dir("/run/php") {
        for e in entries.flatten() {
            if let Some(name) = e.file_name().to_str() {
                if let Some(v) = name
                    .strip_prefix("php")
                    .and_then(|s| s.strip_suffix("-fpm.sock"))
                {
                    out.push(v.to_string());
                }
            }
        }
    }
    // Also look for php-fpm binaries.
    if let Ok(entries) = std::fs::read_dir("/usr/sbin") {
        for e in entries.flatten() {
            if let Some(name) = e.file_name().to_str() {
                if let Some(v) = name
                    .strip_prefix("php")
                    .and_then(|s| s.strip_suffix("-fpm"))
                {
                    if !out.iter().any(|x| x == v) {
                        out.push(v.to_string());
                    }
                }
            }
        }
    }
    out.sort();
    out
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
