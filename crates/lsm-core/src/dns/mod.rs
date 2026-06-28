//! DNS helpers (specs §DNS).
//!
//! Default DNS strategy is dnsmasq plus systemd-resolved routing for development
//! TLDs. This module generates drop-in config and human setup guides for
//! dnsmasq, /etc/hosts, and wildcard setups.

/// dnsmasq drop-in mapping a TLD to localhost.
///
/// e.g. `tld = "test"` -> `address=/test/127.0.0.1`
pub fn dnsmasq_snippet(tld: &str, ip: &str) -> String {
    format!(
        "# Local Site Manager — wildcard DNS for .{tld}\n\
         address=/.{tld}/{ip}\n\
         listen-address=127.0.0.1\n\
         port=5353\n\
         local=/{tld}/\n"
    )
}

/// Path for the dnsmasq drop-in.
pub fn dnsmasq_target() -> String {
    "/etc/dnsmasq.d/local-site-manager.conf".to_string()
}

pub fn resolved_target() -> String {
    "/etc/systemd/resolved.conf.d/local-site-manager.conf".to_string()
}

pub fn resolved_snippet(tld: &str) -> String {
    format!(
        "# Local Site Manager — route .{tld} to local dnsmasq\n\
         [Resolve]\n\
         DNS=127.0.0.1:5353\n\
         Domains=~{tld}\n"
    )
}

/// Markdown guide for the dnsmasq path.
pub fn guide_dnsmasq(tld: &str, ip: &str) -> String {
    format!(
        "# dnsmasq + systemd-resolved setup (recommended)\n\n\
         Map all `.{tld}` names to {ip} through dnsmasq on localhost port 5353, then route only the development TLD with systemd-resolved.\n\n\
         1. Install dnsmasq if it is not already installed:\n\n   \
         ```sh\n   sudo apt install dnsmasq\n   ```\n\n\
         2. Create the dnsmasq drop-in:\n\n   \
         ```sh\n   sudo tee {target} <<'EOF'\n{snippet}EOF\n   ```\n\n\
         3. Create the systemd-resolved route:\n\n   \
         ```sh\n   sudo mkdir -p /etc/systemd/resolved.conf.d\n   sudo tee {resolved_target} <<'EOF'\n{resolved_snippet}EOF\n   ```\n\n\
         4. Restart and flush caches:\n\n   \
         ```sh\n   sudo systemctl restart dnsmasq\n   sudo systemctl restart systemd-resolved\n   sudo resolvectl flush-caches\n   ```\n",
        tld = tld,
        ip = ip,
        target = dnsmasq_target(),
        snippet = dnsmasq_snippet(tld, ip),
        resolved_target = resolved_target(),
        resolved_snippet = resolved_snippet(tld),
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
         - **systemd-resolved**: `Domains=~{tld}` routes only this development TLD to dnsmasq on `127.0.0.1:5353`.\n\
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
