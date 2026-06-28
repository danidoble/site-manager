//! Local Site Manager — privileged root worker.
//!
//! Invoked via `pkexec local-site-manager-privileged '<json>'` (or directly with
//! `--dry-run` for testing). Reads one [`PrivilegedCommand`]-shaped JSON object,
//! executes it as root, and prints a [`PrivilegedResult`] JSON object to stdout.
//! Every action is logged to stderr.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum PrivilegedCommand {
    NginxTest,
    NginxReload,
    WriteNginxConfig {
        target_path: String,
        symlink_path: Option<String>,
        content: String,
    },
    RemoveNginxConfig {
        target_path: String,
        symlink_path: Option<String>,
    },
    InstallCaSystem {
        cert_path: String,
    },
    InstallCaBrowser {
        browser: String,
        cert_path: String,
    },
    Chown {
        path: String,
        owner: String,
        group: Option<String>,
    },
    SetDnsmasq {
        target_path: String,
        content: String,
    },
    SetResolved {
        target_path: String,
        content: String,
    },
    EnsureDir {
        path: String,
    },
    AddHosts {
        site_name: String,
        domains: Vec<String>,
    },
    RemoveHosts {
        site_name: String,
    },
    InstallAutoRenewTimer {
        service_content: String,
        timer_content: String,
    },
    Systemctl {
        action: String,
        service: String,
    },
}

#[derive(Debug, Serialize)]
struct PrivilegedResult {
    success: bool,
    message: String,
    #[serde(default)]
    stdout: String,
    #[serde(default)]
    stderr: String,
}

impl PrivilegedResult {
    fn ok(msg: impl Into<String>) -> Self {
        Self {
            success: true,
            message: msg.into(),
            stdout: String::new(),
            stderr: String::new(),
        }
    }
    fn fail(msg: impl Into<String>) -> Self {
        Self {
            success: false,
            message: msg.into(),
            stdout: String::new(),
            stderr: String::new(),
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dry_run = args.first().map(|a| a == "--dry-run").unwrap_or(false);
    let payload = if dry_run { args.get(1) } else { args.first() };

    let payload = match payload {
        Some(p) => p,
        None => {
            emit(PrivilegedResult::fail("missing JSON command argument"));
            std::process::exit(2);
        }
    };

    let cmd: PrivilegedCommand = match serde_json::from_str(payload) {
        Ok(c) => c,
        Err(e) => {
            emit(PrivilegedResult::fail(format!("invalid command JSON: {e}")));
            std::process::exit(2);
        }
    };

    eprintln!("[lsm-privileged] dry_run={dry_run} op={}", op_name(&cmd));
    let result = if dry_run {
        dry_run_result(&cmd)
    } else {
        execute(&cmd)
    };
    emit(result);
}

fn op_name(c: &PrivilegedCommand) -> &'static str {
    match c {
        PrivilegedCommand::NginxTest => "nginx_test",
        PrivilegedCommand::NginxReload => "nginx_reload",
        PrivilegedCommand::WriteNginxConfig { .. } => "write_nginx_config",
        PrivilegedCommand::RemoveNginxConfig { .. } => "remove_nginx_config",
        PrivilegedCommand::InstallCaSystem { .. } => "install_ca_system",
        PrivilegedCommand::InstallCaBrowser { .. } => "install_ca_browser",
        PrivilegedCommand::Chown { .. } => "chown",
        PrivilegedCommand::SetDnsmasq { .. } => "set_dnsmasq",
        PrivilegedCommand::SetResolved { .. } => "set_resolved",
        PrivilegedCommand::EnsureDir { .. } => "ensure_dir",
        PrivilegedCommand::AddHosts { .. } => "add_hosts",
        PrivilegedCommand::RemoveHosts { .. } => "remove_hosts",
        PrivilegedCommand::InstallAutoRenewTimer { .. } => "install_auto_renew_timer",
        PrivilegedCommand::Systemctl { .. } => "systemctl",
    }
}

fn dry_run_result(c: &PrivilegedCommand) -> PrivilegedResult {
    let msg = match c {
        PrivilegedCommand::NginxTest => "would run `nginx -t`".into(),
        PrivilegedCommand::NginxReload => "would run `systemctl reload nginx`".into(),
        PrivilegedCommand::WriteNginxConfig {
            target_path,
            symlink_path,
            ..
        } => {
            format!(
                "would write {target_path}{}",
                symlink_path
                    .as_deref()
                    .map(|s| format!(" + symlink {s}"))
                    .unwrap_or_default()
            )
        }
        PrivilegedCommand::RemoveNginxConfig {
            target_path,
            symlink_path,
        } => {
            format!(
                "would remove {target_path}{}",
                symlink_path
                    .as_deref()
                    .map(|s| format!(" + symlink {s}"))
                    .unwrap_or_default()
            )
        }
        PrivilegedCommand::InstallCaSystem { cert_path } => {
            format!("would install {cert_path} into system trust store (update-ca-certificates)")
        }
        PrivilegedCommand::InstallCaBrowser { browser, cert_path } => {
            format!("would install {cert_path} into {browser} NSS database")
        }
        PrivilegedCommand::Chown { path, owner, group } => {
            format!(
                "would chown {path} to {owner}{}",
                group
                    .as_deref()
                    .map(|g| format!(":{g}"))
                    .unwrap_or_default()
            )
        }
        PrivilegedCommand::SetDnsmasq { target_path, .. } => {
            format!("would write dnsmasq drop-in {target_path}")
        }
        PrivilegedCommand::SetResolved { target_path, .. } => {
            format!("would write systemd-resolved drop-in {target_path}")
        }
        PrivilegedCommand::EnsureDir { path } => format!("would create directory {path}"),
        PrivilegedCommand::AddHosts { site_name, domains } => {
            format!(
                "would add hosts block for {site_name}: {}",
                domains.join(", ")
            )
        }
        PrivilegedCommand::RemoveHosts { site_name } => {
            format!("would remove hosts block for {site_name}")
        }
        PrivilegedCommand::InstallAutoRenewTimer { .. } => {
            "would install systemd units and run `systemctl daemon-reload`".into()
        }
        PrivilegedCommand::Systemctl { action, service } => {
            format!("would run `systemctl {action} {service}`")
        }
    };
    PrivilegedResult::ok(msg)
}

fn execute(c: &PrivilegedCommand) -> PrivilegedResult {
    match c {
        PrivilegedCommand::NginxTest => run_cmd("nginx", &["-t"], "nginx config test"),
        PrivilegedCommand::NginxReload => {
            run_cmd("systemctl", &["reload", "nginx"], "nginx reload")
        }
        PrivilegedCommand::WriteNginxConfig {
            target_path,
            symlink_path,
            content,
        } => write_nginx_config(target_path, symlink_path.as_deref(), content),
        PrivilegedCommand::RemoveNginxConfig {
            target_path,
            symlink_path,
        } => remove_nginx_config(target_path, symlink_path.as_deref()),
        PrivilegedCommand::InstallCaSystem { cert_path } => install_ca_system(cert_path),
        PrivilegedCommand::InstallCaBrowser { browser, cert_path } => {
            install_ca_browser(browser, cert_path)
        }
        PrivilegedCommand::Chown { path, owner, group } => chown(path, owner, group.as_deref()),
        PrivilegedCommand::SetDnsmasq {
            target_path,
            content,
        } => write_file_idempotent(target_path, content, "dnsmasq drop-in"),
        PrivilegedCommand::SetResolved {
            target_path,
            content,
        } => write_file_idempotent(target_path, content, "systemd-resolved drop-in"),
        PrivilegedCommand::EnsureDir { path } => ensure_dir(path),
        PrivilegedCommand::AddHosts { site_name, domains } => add_hosts(site_name, domains),
        PrivilegedCommand::RemoveHosts { site_name } => remove_hosts(site_name),
        PrivilegedCommand::InstallAutoRenewTimer {
            service_content,
            timer_content,
        } => install_auto_renew_timer(service_content, timer_content),
        PrivilegedCommand::Systemctl { action, service } => systemctl(action, service),
    }
}

fn run_cmd(bin: &str, args: &[&str], label: &str) -> PrivilegedResult {
    let out = match Command::new(bin).args(args).output() {
        Ok(o) => o,
        Err(e) => return PrivilegedResult::fail(format!("{label}: spawn {bin}: {e}")),
    };
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    if out.status.success() {
        PrivilegedResult {
            success: true,
            message: format!("{label} ok"),
            stdout,
            stderr,
        }
    } else {
        PrivilegedResult {
            success: false,
            message: format!(
                "{label} failed (exit {}) {}",
                out.status.code().unwrap_or(-1),
                stderr.trim()
            ),
            stdout,
            stderr,
        }
    }
}

fn write_file(path: &str, content: &str, label: &str) -> PrivilegedResult {
    let p = Path::new(path);
    if let Some(parent) = p.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            return PrivilegedResult::fail(format!("{label}: mkdir parent: {e}"));
        }
    }
    let tmp = p.with_extension("tmp");
    if let Err(e) = fs::write(&tmp, content) {
        return PrivilegedResult::fail(format!("{label}: write tmp: {e}"));
    }
    if let Err(e) = fs::rename(&tmp, p) {
        return PrivilegedResult::fail(format!("{label}: rename: {e}"));
    }
    PrivilegedResult::ok(format!("{label} written to {path}"))
}

fn write_file_idempotent(path: &str, content: &str, label: &str) -> PrivilegedResult {
    if let Ok(existing) = fs::read_to_string(path) {
        if existing == content {
            return PrivilegedResult::ok(format!("{label} already exists at {path}"));
        }
    }
    write_file(path, content, label)
}

fn ensure_dir(path: &str) -> PrivilegedResult {
    match fs::create_dir_all(path) {
        Ok(_) => PrivilegedResult::ok(format!("directory ready at {path}")),
        Err(e) => PrivilegedResult::fail(format!("mkdir -p {path}: {e}")),
    }
}

fn write_nginx_config(
    target_path: &str,
    symlink_path: Option<&str>,
    content: &str,
) -> PrivilegedResult {
    let r = write_file(target_path, content, "nginx config");
    if !r.success {
        return r;
    }
    if let Some(sym) = symlink_path {
        let _ = fs::remove_file(sym);
        #[cfg(unix)]
        {
            if let Err(e) = std::os::unix::fs::symlink(target_path, sym) {
                return PrivilegedResult::fail(format!("symlink {sym}: {e}"));
            }
        }
    }
    PrivilegedResult::ok(format!("nginx config written to {target_path}"))
}

fn remove_nginx_config(target_path: &str, symlink_path: Option<&str>) -> PrivilegedResult {
    if let Some(sym) = symlink_path {
        let _ = fs::remove_file(sym);
    }
    let _ = fs::remove_file(target_path);
    PrivilegedResult::ok(format!("removed {target_path}"))
}

fn install_ca_system(cert_path: &str) -> PrivilegedResult {
    let dest = "/usr/local/share/ca-certificates/lsm-local-ca.crt";
    if let Err(e) = fs::copy(cert_path, dest) {
        return PrivilegedResult::fail(format!("copy CA to {dest}: {e}"));
    }
    // Ensure readable.
    if let Ok(meta) = fs::metadata(dest) {
        let mut perms = meta.permissions();
        perms.set_mode(0o644);
        let _ = fs::set_permissions(dest, perms);
    }
    run_cmd("update-ca-certificates", &[], "install system CA")
}

fn install_ca_browser(browser: &str, cert_path: &str) -> PrivilegedResult {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
    let db = match browser.to_ascii_lowercase().as_str() {
        "chrome" | "chromium" | "brave" => format!("{home}/.pki/nssdb"),
        "firefox" => format!("{home}/.mozilla/firefox"),
        other => return PrivilegedResult::fail(format!("unknown browser `{other}`")),
    };
    // Ensure NSS db exists.
    let _ = fs::create_dir_all(&db);
    // certutil -d sql:<db> -A -n LSM-Local-CA -t "C,," -i <cert>
    let db_arg = format!("sql:{db}");
    let out = Command::new("certutil")
        .args([
            "-d",
            &db_arg,
            "-A",
            "-n",
            "LSM-Local-CA",
            "-t",
            "C,,",
            "-i",
            cert_path,
        ])
        .output();
    match out {
        Ok(o) if o.status.success() => {
            PrivilegedResult::ok(format!("CA installed into {browser} ({db})"))
        }
        Ok(o) => PrivilegedResult::fail(format!(
            "certutil failed: {}",
            String::from_utf8_lossy(&o.stderr).trim()
        )),
        Err(e) => {
            PrivilegedResult::fail(format!("spawn certutil: {e} (is libnss3-tools installed?)"))
        }
    }
}

fn chown(path: &str, owner: &str, group: Option<&str>) -> PrivilegedResult {
    let spec = match group {
        Some(g) => format!("{owner}:{g}"),
        None => owner.to_string(),
    };
    run_cmd("chown", &[&spec, path], &format!("chown {spec}"))
}

fn add_hosts(site_name: &str, domains: &[String]) -> PrivilegedResult {
    let clean: Vec<String> = domains
        .iter()
        .map(|d| d.trim().trim_start_matches("*.").to_string())
        .filter(|d| !d.is_empty() && !d.contains(char::is_whitespace))
        .collect();
    if clean.is_empty() {
        return PrivilegedResult::fail("hosts: no valid domains");
    }
    let existing = fs::read_to_string("/etc/hosts").unwrap_or_default();
    let stripped = strip_hosts_block(&existing, site_name);
    let block = format!(
        "\n# local-site-manager begin {site_name}\n127.0.0.1 {}\n::1 {}\n# local-site-manager end {site_name}\n",
        clean.join(" "),
        clean.join(" ")
    );
    write_file(
        "/etc/hosts",
        &(stripped.trim_end().to_string() + &block),
        "hosts",
    )
}

fn remove_hosts(site_name: &str) -> PrivilegedResult {
    let existing = fs::read_to_string("/etc/hosts").unwrap_or_default();
    write_file(
        "/etc/hosts",
        &strip_hosts_block(&existing, site_name),
        "hosts",
    )
}

fn install_auto_renew_timer(service_content: &str, timer_content: &str) -> PrivilegedResult {
    let service_path = "/etc/systemd/system/local-site-manager.service";
    let timer_path = "/etc/systemd/system/local-site-manager.timer";

    let service = write_file(service_path, service_content, "systemd service");
    if !service.success {
        return service;
    }
    let timer = write_file(timer_path, timer_content, "systemd timer");
    if !timer.success {
        return timer;
    }

    for path in [service_path, timer_path] {
        if let Ok(meta) = fs::metadata(path) {
            let mut perms = meta.permissions();
            perms.set_mode(0o644);
            let _ = fs::set_permissions(path, perms);
        }
    }

    let reload = run_cmd("systemctl", &["daemon-reload"], "systemctl daemon-reload");
    if !reload.success {
        return reload;
    }
    PrivilegedResult::ok("auto-renew timer installed")
}

fn strip_hosts_block(text: &str, site_name: &str) -> String {
    let begin = format!("# local-site-manager begin {site_name}");
    let end = format!("# local-site-manager end {site_name}");
    let mut out = Vec::new();
    let mut skipping = false;
    for line in text.lines() {
        if line.trim() == begin {
            skipping = true;
            continue;
        }
        if skipping && line.trim() == end {
            skipping = false;
            continue;
        }
        if !skipping {
            out.push(line);
        }
    }
    out.join("\n") + "\n"
}

fn systemctl(action: &str, service: &str) -> PrivilegedResult {
    let allowed_action = matches!(
        action,
        "reload"
            | "restart"
            | "status"
            | "is-active"
            | "is-enabled"
            | "enable"
            | "disable"
            | "start"
            | "stop"
    );
    let allowed_service = service == "nginx"
        || service == "dnsmasq"
        || service == "systemd-resolved"
        || service == "local-site-manager.timer"
        || service == "local-site-manager.service"
        || (service.starts_with("php") && service.ends_with("-fpm"));
    if !allowed_action || !allowed_service {
        return PrivilegedResult::fail(format!("systemctl {action} {service}: not allowed"));
    }
    run_cmd(
        "systemctl",
        &[action, service],
        &format!("systemctl {action} {service}"),
    )
}

fn emit(result: PrivilegedResult) {
    println!(
        "{}",
        serde_json::to_string(&result).expect("serialize result")
    );
}
