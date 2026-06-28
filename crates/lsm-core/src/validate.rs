//! Input validation — domains, site names, paths, proxy targets.
//!
//! Per specs §Security: validate domains, validate paths, prevent command injection.
//! Every shell-out elsewhere uses `Command` arg vectors; this module guards the
//! free-form text inputs that reach those vectors.

use std::path::Path;

use crate::error::{Error, Result};

/// Validate a domain (or wildcard domain). Rejects empty, `..`, slashes, spaces.
pub fn validate_domain(domain: &str) -> Result<()> {
    let d = domain.trim();
    if d.is_empty() {
        return Err(Error::Validation("domain is empty".into()));
    }
    if d.contains('/') || d.contains(' ') || d.contains("..") {
        return Err(Error::Validation(format!("invalid domain: {domain}")));
    }
    if d.matches('*').count() > 1 {
        return Err(Error::Validation(format!(
            "invalid domain (only one wildcard label allowed): {domain}"
        )));
    }
    let s = d.strip_prefix("*.").unwrap_or(d);
    if s.is_empty() {
        return Err(Error::Validation(format!("invalid domain: {domain}")));
    }
    for label in s.split('.') {
        if label.is_empty() || label.len() > 63 {
            return Err(Error::Validation(format!("invalid domain: {domain}")));
        }
        if !label.chars().all(is_label_char) {
            return Err(Error::Validation(format!("invalid domain: {domain}")));
        }
        if !label.starts_with(|c: char| c.is_ascii_alphanumeric())
            || !label.ends_with(|c: char| c.is_ascii_alphanumeric())
        {
            return Err(Error::Validation(format!("invalid domain: {domain}")));
        }
    }
    Ok(())
}

fn is_label_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-'
}

/// Validate a site name: lowercase alnum, dash, underscore; 1..=63 chars.
pub fn validate_site_name(name: &str) -> Result<()> {
    let n = name.trim();
    if n.is_empty() || n.len() > 63 {
        return Err(Error::Validation(format!("invalid site name: {name}")));
    }
    if !n.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit()) {
        return Err(Error::Validation(format!("invalid site name: {name}")));
    }
    if !n
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        return Err(Error::Validation(format!("invalid site name: {name}")));
    }
    Ok(())
}

/// Validate that `path` is sane. Rejects `..` segments. When `require_within_root`
/// is set the path must resolve under `root`.
pub fn validate_path(path: &str, root: &Path, require_within_root: bool) -> Result<()> {
    let p = Path::new(path);
    for comp in p.components() {
        use std::path::Component;
        if let Component::ParentDir = comp {
            return Err(Error::Validation(format!(
                "path contains parent segment: {path}"
            )));
        }
    }
    if require_within_root {
        let abs = if p.is_absolute() {
            p.to_path_buf()
        } else {
            root.join(p)
        };
        let root_abs = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        if !abs.starts_with(&root_abs) {
            return Err(Error::Validation(format!(
                "path {path} outside of allowed root {}",
                root_abs.display()
            )));
        }
    }
    Ok(())
}

/// Validate a `host:port` proxy target. Rejects shell metacharacters.
pub fn validate_proxy_target(target: &str) -> Result<()> {
    let t = target.trim();
    if t.is_empty() {
        return Err(Error::Validation("proxy target is empty".into()));
    }
    if t.contains(|c: char| c.is_whitespace() || ";|&$`<>(){}".contains(c)) {
        return Err(Error::Validation(format!(
            "proxy target has invalid characters: {target}"
        )));
    }
    let (host, port) = match t.rsplit_once(':') {
        Some((h, p)) => (h, p),
        None => {
            return Err(Error::Validation(format!(
                "proxy target missing :port: {target}"
            )))
        }
    };
    if host.is_empty() || port.is_empty() {
        return Err(Error::Validation(format!("invalid proxy target: {target}")));
    }
    let port_n: u32 = port
        .parse()
        .map_err(|_| Error::Validation(format!("invalid port: {port}")))?;
    if port_n == 0 || port_n > 65535 {
        return Err(Error::Validation(format!("port out of range: {port}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domains() {
        assert!(validate_domain("app.test").is_ok());
        assert!(validate_domain("*.test").is_ok());
        assert!(validate_domain("a.b.test").is_ok());
        assert!(validate_domain("app..test").is_err());
        assert!(validate_domain("App Test").is_err());
        assert!(validate_domain("/etc/passwd").is_err());
        assert!(validate_domain("*.*.test").is_err());
    }

    #[test]
    fn names() {
        assert!(validate_site_name("app").is_ok());
        assert!(validate_site_name("my_app-1").is_ok());
        assert!(validate_site_name("App").is_err());
        assert!(validate_site_name("a b").is_err());
        assert!(validate_site_name("-bad").is_err());
    }

    #[test]
    fn proxy_targets() {
        assert!(validate_proxy_target("127.0.0.1:3000").is_ok());
        assert!(validate_proxy_target("localhost:3000").is_ok());
        assert!(validate_proxy_target("127.0.0.1").is_err());
        assert!(validate_proxy_target("127.0.0.1:;ls").is_err());
        assert!(validate_proxy_target("127.0.0.1:99999").is_err());
    }

    #[test]
    fn paths() {
        let root = std::path::Path::new("/var/www");
        assert!(validate_path("/var/www/app", root, true).is_ok());
        assert!(validate_path("../etc", root, true).is_err());
        assert!(validate_path("/etc/passwd", root, true).is_err());
    }
}
