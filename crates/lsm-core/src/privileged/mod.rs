//! Privileged operations client.
//!
//! Privileged actions (writing under `/etc/nginx`, reloading nginx, installing the
//! CA into system/browser trust stores) are delegated to a separate root worker
//! binary (`lsm-privileged`) spawned via `pkexec`. Commands are exchanged as JSON.
//!
//! When `dry_run` is set we spawn the helper **without** pkexec and with
//! `--dry-run`; the helper prints what it would do and executes nothing — this is
//! the path exercised by tests.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// A privileged command, serialized as `{"op": "<variant>", ...}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum PrivilegedCommand {
    /// `nginx -t` config validation.
    NginxTest,
    /// Reload nginx (`systemctl reload nginx`).
    NginxReload,
    /// Atomically write a config file under `/etc/nginx`, optionally symlinking it.
    WriteNginxConfig {
        target_path: String,
        symlink_path: Option<String>,
        content: String,
    },
    /// Remove a config file and its symlink.
    RemoveNginxConfig {
        target_path: String,
        symlink_path: Option<String>,
    },
    /// Install a CA cert into the system trust store (`update-ca-certificates`).
    InstallCaSystem { cert_path: String },
    /// Install a CA cert into a browser NSS DB (`certutil`).
    InstallCaBrowser { browser: String, cert_path: String },
    /// `chown` a path.
    Chown {
        path: String,
        owner: String,
        group: Option<String>,
    },
    /// Write a dnsmasq config drop-in.
    SetDnsmasq { target_path: String, content: String },
    /// Ensure a directory exists.
    EnsureDir { path: String },
    /// Add an idempotent managed /etc/hosts block for a site.
    AddHosts { site_name: String, domains: Vec<String> },
    /// Remove the managed /etc/hosts block for a site.
    RemoveHosts { site_name: String },
    /// Install the bundled systemd units used for automatic SSL renewal.
    InstallAutoRenewTimer {
        service_content: String,
        timer_content: String,
    },
    /// Run a constrained systemctl action for an allowed local service.
    Systemctl { action: String, service: String },
}

/// Result returned by the privileged helper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivilegedResult {
    pub success: bool,
    pub message: String,
    #[serde(default)]
    pub stdout: String,
    #[serde(default)]
    pub stderr: String,
}

impl PrivilegedResult {
    pub fn ok(message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
            stdout: String::new(),
            stderr: String::new(),
        }
    }
}

/// Execute a privileged command via the helper binary.
///
/// `helper` is the path/name of `local-site-manager-privileged`. When `dry_run` is
/// true the helper is run directly (no pkexec) with `--dry-run`, so it performs no
/// privileged action — safe in any environment.
pub fn run(cmd: &PrivilegedCommand, dry_run: bool, helper: &str) -> Result<PrivilegedResult> {
    let result = run_capture(cmd, dry_run, helper)?;
    if !result.success {
        return Err(Error::Privileged(result.message));
    }
    Ok(result)
}

/// Execute a privileged command and return the helper response even when the
/// underlying operation fails. Useful for checks such as `nginx -t`, where the
/// stderr is the actual diagnostic payload.
pub fn run_capture(cmd: &PrivilegedCommand, dry_run: bool, helper: &str) -> Result<PrivilegedResult> {
    let payload = serde_json::to_string(cmd)?;
    let mut command = if dry_run {
        let mut c = std::process::Command::new(helper);
        c.arg("--dry-run");
        c
    } else {
        let mut c = std::process::Command::new("pkexec");
        c.arg(helper);
        c
    };
    command.arg(&payload);

    let output = command
        .output()
        .map_err(|e| Error::Privileged(format!("spawn helper `{helper}`: {e}")))?;

    if !output.status.success() && output.stdout.is_empty() {
        return Err(Error::Privileged(format!(
            "helper exited {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    let result: PrivilegedResult = serde_json::from_slice(&output.stdout).map_err(|e| {
        Error::Privileged(format!(
            "decode helper response: {e}; stdout: {}; stderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        ))
    })?;
    Ok(result)
}

/// True when the process is effectively running with elevated privileges.
pub fn running_as_root() -> bool {
    #[cfg(unix)]
    {
        unsafe { libc_geteuid() == 0 }
    }
    #[cfg(not(unix))]
    {
        false
    }
}

#[cfg(unix)]
unsafe fn libc_geteuid() -> u32 {
    extern "C" {
        fn geteuid() -> u32;
    }
    geteuid()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_serializes_tagged() {
        let cmd = PrivilegedCommand::NginxReload;
        let s = serde_json::to_string(&cmd).unwrap();
        assert_eq!(s, r#"{"op":"nginx_reload"}"#);
        let cmd = PrivilegedCommand::WriteNginxConfig {
            target_path: "/etc/nginx/sites-available/x.conf".into(),
            symlink_path: Some("/etc/nginx/sites-enabled/x.conf".into()),
            content: "# x".into(),
        };
        let s = serde_json::to_string(&cmd).unwrap();
        assert!(s.contains(r#""op":"write_nginx_config""#));
        assert!(s.contains("target_path"));
    }
}
