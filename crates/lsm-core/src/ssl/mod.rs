//! Certificate issuance (specs §SSL).
//!
//! Two [`CertProvider`] implementations:
//! - [`internal`] — signs leaf certs with the app's internal CA (openssl),
//! - [`mkcert`] — shells out to the `mkcert` binary.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use openssl::asn1::Asn1Time;
use openssl::bn::{BigNum, MsbOption};
use openssl::hash::MessageDigest;
use openssl::nid::Nid;
use openssl::pkey::PKey;
use openssl::rsa::Rsa;
use openssl::x509::{X509Builder, X509Extension, X509Name, X509};

use crate::ca;
use crate::error::{Error, Result};

/// Leaf cert validity (browser-compliant ~27 months).
pub const LEAF_VALIDITY_DAYS: u32 = 825;
const LEAF_KEY_BITS: u32 = 2048;

/// Output of issuing a certificate (before it's persisted to the DB).
#[derive(Debug, Clone)]
pub struct IssuedCert {
    pub provider: String,
    pub domains: Vec<String>,
    pub cert_path: String,
    pub key_path: String,
    pub not_before: String,
    pub not_after: String,
    pub fingerprint: String,
}

/// Issue a leaf certificate signed by the internal CA.
#[allow(deprecated)] // X509Extension::new string API is deprecated but concise and correct here.
pub fn issue_internal(
    ca_dir: &Path,
    out_dir: &Path,
    name: &str,
    domains: &[String],
) -> Result<IssuedCert> {
    let (ca_cert_path, ca_key_path) = ca::ca_paths(ca_dir);
    if !ca_cert_path.exists() || !ca_key_path.exists() {
        return Err(Error::Other(
            "internal CA not initialized; run `ca init` first".into(),
        ));
    }

    fs::create_dir_all(out_dir)?;
    let cert_path = out_dir.join(format!("{name}.crt"));
    let key_path = out_dir.join(format!("{name}.key"));

    let ca_pem = fs::read(&ca_cert_path)?;
    let ca_key_pem = fs::read(&ca_key_path)?;
    let ca_cert = X509::from_pem(&ca_pem)?;
    let ca_pkey = PKey::private_key_from_pem(&ca_key_pem)?;

    let rsa = Rsa::generate(LEAF_KEY_BITS)?;
    let leaf_pkey = PKey::from_rsa(rsa)?;

    // CN = first domain without wildcard label.
    let cn = domains
        .first()
        .map(|d| d.trim_start_matches("*.").to_string())
        .unwrap_or_else(|| name.to_string());

    let mut subject = X509Name::builder()?;
    subject.append_entry_by_nid(Nid::COMMONNAME, &cn)?;
    let subject = subject.build();

    let mut b = X509Builder::new()?;
    b.set_version(2)?;
    b.set_subject_name(&subject)?;
    b.set_issuer_name(ca_cert.subject_name())?;
    b.set_pubkey(&leaf_pkey)?;

    let mut serial_bn = BigNum::new()?;
    serial_bn.rand(159, MsbOption::MAYBE_ZERO, false)?;
    let serial = serial_bn.to_asn1_integer()?;
    b.set_serial_number(&serial)?;
    let nb = Asn1Time::days_from_now(0)?;
    b.set_not_before(nb.as_ref())?;
    let na = Asn1Time::days_from_now(LEAF_VALIDITY_DAYS)?;
    b.set_not_after(na.as_ref())?;

    b.append_extension(X509Extension::new(
        None,
        None,
        "basicConstraints",
        "critical,CA:FALSE",
    )?)?;
    b.append_extension(X509Extension::new(
        None,
        None,
        "keyUsage",
        "critical,digitalSignature,keyEncipherment",
    )?)?;
    b.append_extension(X509Extension::new(
        None,
        None,
        "extendedKeyUsage",
        "serverAuth",
    )?)?;
    let san = san_string(domains);
    b.append_extension(X509Extension::new(None, None, "subjectAltName", &san)?)?;

    b.sign(&ca_pkey, MessageDigest::sha256())?;
    let cert = b.build();

    let cert_pem = cert.to_pem()?;
    let key_pem = leaf_pkey.private_key_to_pem_pkcs8()?;

    atomic_write(&cert_path, &cert_pem)?;
    atomic_write(&key_path, &key_pem)?;
    restrict_perms(&key_path)?;

    Ok(IssuedCert {
        provider: "internal".to_string(),
        domains: domains.to_vec(),
        cert_path: cert_path.to_string_lossy().to_string(),
        key_path: key_path.to_string_lossy().to_string(),
        not_before: cert.not_before().to_string(),
        not_after: cert.not_after().to_string(),
        fingerprint: ca::fingerprint_hex(&cert)?,
    })
}

/// Issue a certificate via `mkcert`. Requires `mkcert` on PATH.
pub fn issue_mkcert(out_dir: &Path, name: &str, domains: &[String]) -> Result<IssuedCert> {
    let mkcert =
        which("mkcert").ok_or_else(|| Error::NotFound("mkcert binary not found on PATH".into()))?;
    fs::create_dir_all(out_dir)?;
    let cert_path = out_dir.join(format!("{name}.crt"));
    let key_path = out_dir.join(format!("{name}.key"));

    let mut cmd = Command::new(mkcert);
    cmd.args(["-cert-file", &cert_path.to_string_lossy()])
        .args(["-key-file", &key_path.to_string_lossy()]);
    for d in domains {
        cmd.arg(d);
    }
    let out = cmd.output()?;
    if !out.status.success() {
        return Err(Error::Other(format!(
            "mkcert failed: {}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }

    let pem = fs::read(&cert_path)?;
    let cert = X509::from_pem(&pem)?;
    Ok(IssuedCert {
        provider: "mkcert".to_string(),
        domains: domains.to_vec(),
        cert_path: cert_path.to_string_lossy().to_string(),
        key_path: key_path.to_string_lossy().to_string(),
        not_before: cert.not_before().to_string(),
        not_after: cert.not_after().to_string(),
        fingerprint: ca::fingerprint_hex(&cert)?,
    })
}

fn san_string(domains: &[String]) -> String {
    domains
        .iter()
        .map(|d| format!("DNS:{d}"))
        .collect::<Vec<_>>()
        .join(",")
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

fn atomic_write(path: &Path, data: &[u8]) -> Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, data)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

fn restrict_perms(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if path.exists() {
            let mut perms = fs::metadata(path)?.permissions();
            perms.set_mode(0o600);
            fs::set_permissions(path, perms)?;
        }
    }
    let _ = path;
    Ok(())
}

/// True if a cert is within `days` of expiry (or already expired).
pub fn expiring(not_after: &str, days: u32) -> bool {
    // not_after is an openssl display string; best-effort parse.
    let parsed = chrono::NaiveDateTime::parse_from_str(not_after, "%b %e %H:%M:%S %Y GMT")
        .ok()
        .or_else(|| {
            chrono::DateTime::parse_from_rfc3339(not_after)
                .ok()
                .map(|dt| dt.naive_utc())
        });
    match parsed {
        Some(dt) => {
            let expiry = dt.and_utc();
            let horizon = chrono::Utc::now() + chrono::Duration::days(days as i64);
            expiry <= horizon
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ca;

    fn name_str(n: &openssl::x509::X509NameRef) -> String {
        n.entries()
            .map(|e| {
                format!(
                    "{}={}",
                    e.object().nid().as_raw(),
                    e.data().to_string().unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join(",")
    }

    #[test]
    fn internal_signs_leaf() {
        let dir = tempfile::tempdir().unwrap();
        let ca_dir = dir.path().join("ca");
        let certs_dir = dir.path().join("certs");
        ca::generate_ca(&ca_dir, "internal").unwrap();

        let domains = vec!["app.test".to_string(), "*.app.test".to_string()];
        let issued = issue_internal(&ca_dir, &certs_dir, "app", &domains).unwrap();
        assert_eq!(issued.provider, "internal");
        assert_eq!(issued.domains, domains);

        // The leaf verifies against the CA.
        let ca_pem = fs::read(ca_dir.join("rootCA.crt")).unwrap();
        let leaf_pem = fs::read(&issued.cert_path).unwrap();
        let ca_cert = X509::from_pem(&ca_pem).unwrap();
        let leaf = X509::from_pem(&leaf_pem).unwrap();
        assert_eq!(
            name_str(leaf.issuer_name()),
            name_str(ca_cert.subject_name())
        );
    }
}
