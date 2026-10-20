//! The Remote server's self-signed certificate (ADR 0002). Phones pin its fingerprint during
//! Pairing, so a new certificate means every phone must pair again.

use std::{io, path::Path};

use rustls::pki_types::{CertificateDer, pem::PemObject};
use sha2::{Digest, Sha256};

pub struct Identity {
    pub cert_pem: Vec<u8>,
    pub key_pem: Vec<u8>,
    /// Lowercase hex SHA-256 of the certificate's DER, which is what a phone pins.
    pub fingerprint: String,
}

fn fingerprint(cert_pem: &[u8]) -> Option<String> {
    let der = CertificateDer::from_pem_slice(cert_pem).ok()?;
    Some(Sha256::digest(der.as_ref()).iter().map(|b| format!("{b:02x}")).collect())
}

/// The certificate stored in `dir`, or a new one if there is none (or it is damaged). The flag
/// says a new one was made.
pub fn load_or_create(dir: &Path) -> io::Result<(Identity, bool)> {
    let (cert_path, key_path) = (dir.join("cert.pem"), dir.join("key.pem"));
    if let (Ok(cert_pem), Ok(key_pem)) = (std::fs::read(&cert_path), std::fs::read(&key_path))
        && let Some(fingerprint) = fingerprint(&cert_pem)
    {
        return Ok((Identity { cert_pem, key_pem, fingerprint }, false));
    }
    let names = ["m4tv.local", "localhost", "127.0.0.1"].map(String::from).to_vec();
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(names).map_err(io::Error::other)?;
    let (cert_pem, key_pem) = (cert.pem().into_bytes(), signing_key.serialize_pem().into_bytes());
    std::fs::create_dir_all(dir)?;
    std::fs::write(&cert_path, &cert_pem)?;
    std::fs::write(&key_path, &key_pem)?;
    restrict(&key_path)?;
    let fingerprint =
        fingerprint(&cert_pem).ok_or_else(|| io::Error::other("unreadable certificate"))?;
    Ok((Identity { cert_pem, key_pem, fingerprint }, true))
}

/// Keys and tokens are the Remote server's secrets: only the file's owner reads them.
#[cfg(unix)]
pub(super) fn restrict(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
pub(super) fn restrict(_path: &Path) -> io::Result<()> {
    Ok(())
}
