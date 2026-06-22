//! Internal certificate authority (specs §SSL → Local CA).
//!
//! Generates a 30-year self-signed root CA (RSA 4096) stored under the app's
//! `ca/` directory. Leaf certificate signing lives in [`crate::ssl`].

use std::fs;
use std::path::{Path, PathBuf};

use openssl::asn1::Asn1Time;
use openssl::bn::{BigNum, MsbOption};
use openssl::hash::{hash, MessageDigest};
use openssl::nid::Nid;
use openssl::pkey::PKey;
use openssl::rsa::Rsa;
use openssl::x509::{X509, X509Builder, X509Name};

use crate::domain::Ca;
use crate::error::Result;

/// CA validity in days (30 years).
pub const CA_VALIDITY_DAYS: u32 = 30 * 365 + 7;
const CA_KEY_BITS: u32 = 4096;
const CA_SUBJECT: &str = "Local Site Manager Local CA";

/// Where the CA material lives inside a storage root.
pub fn ca_paths(ca_dir: &Path) -> (PathBuf, PathBuf) {
    (
        ca_dir.join("rootCA.crt"),
        ca_dir.join("rootCA.key"),
    )
}

/// Generate (or overwrite) the internal CA at `ca_dir`.
pub fn generate_ca(ca_dir: &Path, provider: &str) -> Result<Ca> {
    fs::create_dir_all(ca_dir)?;
    let (cert_path, key_path) = ca_paths(ca_dir);

    let rsa = Rsa::generate(CA_KEY_BITS)?;
    let pkey = PKey::from_rsa(rsa)?;

    let mut name = X509Name::builder()?;
    name.append_entry_by_nid(Nid::COMMONNAME, CA_SUBJECT)?;
    name.append_entry_by_nid(Nid::ORGANIZATIONNAME, "Local Site Manager")?;
    let name = name.build();

    let mut b = X509Builder::new()?;
    b.set_version(2)?;
    b.set_subject_name(&name)?;
    b.set_issuer_name(&name)?;
    b.set_pubkey(&pkey)?;

    let mut serial_bn = BigNum::new()?;
    serial_bn.rand(159, MsbOption::MAYBE_ZERO, false)?;
    let serial = serial_bn.to_asn1_integer()?;
    b.set_serial_number(&serial)?;

    let nb = Asn1Time::days_from_now(0)?;
    b.set_not_before(nb.as_ref())?;
    let na = Asn1Time::days_from_now(CA_VALIDITY_DAYS)?;
    b.set_not_after(na.as_ref())?;

    b.sign(&pkey, MessageDigest::sha256())?;
    let cert = b.build();

    let cert_pem = cert.to_pem()?;
    let key_pem = pkey.private_key_to_pem_pkcs8()?;

    fs::write(&cert_path, &cert_pem)?;
    restrict_key_perms(&key_path)?;
    fs::write(&key_path, &key_pem)?;
    restrict_key_perms(&key_path)?;

    let fingerprint = fingerprint_hex(&cert)?;

    Ok(Ca {
        id: 0,
        provider: provider.to_string(),
        name: CA_SUBJECT.to_string(),
        cert_path: cert_path.to_string_lossy().to_string(),
        key_path: key_path.to_string_lossy().to_string(),
        fingerprint,
        created_at: now_rfc3339(),
    })
}

/// Load CA material if present.
pub fn load_ca(ca_dir: &Path, provider: &str) -> Result<Option<Ca>> {
    let (cert_path, key_path) = ca_paths(ca_dir);
    if !cert_path.exists() || !key_path.exists() {
        return Ok(None);
    }
    let cert_pem = fs::read(&cert_path)?;
    let cert = X509::from_pem(&cert_pem)?;
    let fingerprint = fingerprint_hex(&cert)?;
    Ok(Some(Ca {
        id: 0,
        provider: provider.to_string(),
        name: CA_SUBJECT.to_string(),
        cert_path: cert_path.to_string_lossy().to_string(),
        key_path: key_path.to_string_lossy().to_string(),
        fingerprint,
        created_at: now_rfc3339(),
    }))
}

/// SHA-256 fingerprint of a cert as lowercase hex.
pub fn fingerprint_hex(cert: &X509) -> Result<String> {
    let der = cert.to_der()?;
    let digest = hash(MessageDigest::sha256(), &der)?;
    Ok(digest.iter().map(|b| format!("{b:02x}")).collect())
}

/// Write with `600` perms; best-effort on non-unix.
fn restrict_key_perms(path: &Path) -> Result<()> {
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

fn now_rfc3339() -> String {
    // chrono avoids depending on system clock weirdness in tests; use UTC now.
    chrono::Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name_str(n: &openssl::x509::X509NameRef) -> String {
        n.entries()
            .map(|e| format!("{}={}", e.object().nid().as_raw(), e.data().to_string().unwrap_or_default()))
            .collect::<Vec<_>>()
            .join(",")
    }

    #[test]
    fn generate_then_load() {
        let dir = tempfile::tempdir().unwrap();
        let ca = generate_ca(dir.path(), "internal").unwrap();
        assert!(ca.cert_path.ends_with("rootCA.crt"));
        assert_eq!(ca.fingerprint.len(), 64);

        let loaded = load_ca(dir.path(), "internal").unwrap().unwrap();
        assert_eq!(loaded.fingerprint, ca.fingerprint);
    }

    #[test]
    fn ca_cert_self_signed() {
        let dir = tempfile::tempdir().unwrap();
        let _ca = generate_ca(dir.path(), "internal").unwrap();
        let pem = fs::read(dir.path().join("rootCA.crt")).unwrap();
        let cert = X509::from_pem(&pem).unwrap();
        // subject == issuer for a self-signed root
        let subj = name_str(cert.subject_name());
        let issuer = name_str(cert.issuer_name());
        assert_eq!(subj, issuer);
        assert!(subj.contains("Local Site Manager Local CA"));
    }
}
