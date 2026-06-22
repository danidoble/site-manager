//! DNS helpers (specs §DNS).
//!
//! Default DNS strategy is dnsmasq (no automatic /etc/hosts edits). This module
//! generates drop-in config and human setup guides for dnsmasq, /etc/hosts, and
//! wildcard setups.

/// dnsmasq drop-in mapping a TLD to localhost.
///
/// e.g. `tld = "test"` -> `address=/test/127.0.0.1`
pub fn dnsmasq_snippet(tld: &str, ip: &str) -> String {
    format!(
        "# Local Site Manager — wildcard DNS for .{tld}\n\
         address=/.{tld}/{ip}\n\
         local=/{tld}/\n"
    )
}

/// Path for the dnsmasq drop-in (NetworkManager-aware on Ubuntu).
pub fn dnsmasq_target() -> String {
    // Ubuntu's NetworkManager ships a dnsmasq instance; this path works there.
    "/etc/NetworkManager/dnsmasq.d/local-site-manager.conf".to_string()
}

/// Markdown guide for the dnsmasq path.
pub fn guide_dnsmasq(tld: &str, ip: &str) -> String {
    format!(
        "# dnsmasq setup (recommended)\n\n\
         Map all `.{tld}` names to {ip}.\n\n\
         1. Install dnsmasq (or use NetworkManager's built-in instance):\n\n   \
         ```sh\n   sudo apt install dnsmasq\n   ```\n\n\
         2. Create the drop-in:\n\n   \
         ```sh\n   sudo tee {target} <<'EOF'\n{snippet}EOF\n   ```\n\n\
         3. Restart the resolver:\n\n   \
         ```sh\n   sudo systemctl restart NetworkManager\n   # or: sudo systemctl restart dnsmasq\n   ```\n",
        tld = tld,
        ip = ip,
        target = dnsmasq_target(),
        snippet = dnsmasq_snippet(tld, ip),
    )
}

/// Markdown guide for the /etc/hosts path (no wildcards).
pub fn guide_hosts(domain: &str, ip: &str) -> String {
    format!(
        "# /etc/hosts setup (per-domain, no wildcards)\n\n\
         Add a line for each domain:\n\n\
         ```sh\n   echo '{ip} {domain}' | sudo tee -a /etc/hosts\n   ```\n",
        domain = domain,
        ip = ip,
    )
}

/// Markdown guide describing wildcard options.
pub fn guide_wildcards(tld: &str) -> String {
    format!(
        "# Wildcard domains\n\n\
         - **dnsmasq**: `address=/.{tld}/127.0.0.1` resolves every `*.{tld}`.\n\
         - **systemd-resolved**: `systemd-resolve --mklift ...` is not used by this app.\n\
         - **/etc/hosts**: cannot express wildcards; list domains explicitly.\n",
        tld = tld,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippet_maps_tld() {
        let s = dnsmasq_snippet("test", "127.0.0.1");
        assert!(s.contains("address=/.test/127.0.0.1"));
        assert!(s.contains("local=/test/"));
    }

    #[test]
    fn guides_mention_target() {
        assert!(guide_dnsmasq("test", "127.0.0.1").contains("dnsmasq"));
        assert!(guide_hosts("app.test", "127.0.0.1").contains("/etc/hosts"));
        assert!(guide_wildcards("test").contains("dnsmasq"));
    }
}
