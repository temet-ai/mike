use std::path::{Path, PathBuf};
use std::sync::Arc;

use rcgen::{BasicConstraints, CertificateParams, DnType, IsCa, Issuer, KeyPair, KeyUsagePurpose};

use crate::error::ForwardError;

const CA_CERT_FILE: &str = "ca-cert.pem";
const CA_KEY_FILE: &str = "ca-key.pem";

pub fn ca_cert_path(dir: &Path) -> PathBuf {
    dir.join(CA_CERT_FILE)
}

pub fn default_ca_cert_path() -> PathBuf {
    ca_cert_path(&crate::config::resolve_ca_dir(None))
}

pub struct Ca {
    issuer: Arc<Issuer<'static, KeyPair>>,
    cert_pem: String,
    cert_der: rustls_pki_types::CertificateDer<'static>,
}

impl Ca {
    pub fn load_or_generate(dir: &Path) -> Result<Self, ForwardError> {
        let cert_path = dir.join(CA_CERT_FILE);
        let key_path = dir.join(CA_KEY_FILE);

        if cert_path.is_file()
            && key_path.is_file()
            && let Some(ca) = Self::try_load_existing(&cert_path, &key_path, dir)
        {
            return Ok(ca);
        }

        Self::generate_and_persist(dir, &cert_path, &key_path)
    }

    fn try_load_existing(cert_path: &Path, key_path: &Path, dir: &Path) -> Option<Self> {
        match Self::load(cert_path, key_path) {
            Ok(ca) => {
                log_ca_loaded(dir);
                Some(ca)
            }
            Err(e) => {
                log_ca_load_failed(&e);
                None
            }
        }
    }

    fn generate_and_persist(
        dir: &Path,
        cert_path: &Path,
        key_path: &Path,
    ) -> Result<Self, ForwardError> {
        let ca = Self::generate()?;
        ca.persist(dir, cert_path, key_path)?;
        tracing::warn!(
            dir = %dir.display(),
            "generated a NEW MITM CA — re-trust it (proxy-forward export-ca) \
             before Claude Code will accept the intercepted TLS"
        );
        Ok(ca)
    }

    fn load(cert_path: &Path, key_path: &Path) -> Result<Self, ForwardError> {
        let cert_pem = std::fs::read_to_string(cert_path)
            .map_err(|e| ForwardError::CaInit(format!("read {}: {e}", cert_path.display())))?;
        let key_pem = std::fs::read_to_string(key_path)
            .map_err(|e| ForwardError::CaInit(format!("read {}: {e}", key_path.display())))?;

        let key_pair = KeyPair::from_pem(&key_pem)
            .map_err(|e| ForwardError::CaInit(format!("parse CA key: {e}")))?;

        let issuer = Issuer::from_ca_cert_pem(&cert_pem, key_pair)
            .map_err(|e| ForwardError::CaInit(format!("rebuild issuer from CA cert: {e}")))?;

        let cert_der = pem_to_cert_der(&cert_pem)?;

        Ok(Self {
            issuer: Arc::new(issuer),
            cert_pem,
            cert_der,
        })
    }

    fn generate() -> Result<Self, ForwardError> {
        let key_pair = KeyPair::generate()
            .map_err(|e| ForwardError::CaInit(format!("generate CA key: {e}")))?;

        let mut params = CertificateParams::default();
        params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
        params
            .distinguished_name
            .push(DnType::CommonName, "temet proxy-forward local CA");
        params
            .distinguished_name
            .push(DnType::OrganizationName, "temet proxy-forward");
        params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
            KeyUsagePurpose::DigitalSignature,
        ];

        let cert = params
            .self_signed(&key_pair)
            .map_err(|e| ForwardError::CaInit(format!("self-sign CA cert: {e}")))?;

        let cert_pem = cert.pem();
        let cert_der: rustls_pki_types::CertificateDer<'static> = cert.der().clone();

        let issuer = Issuer::new(params, key_pair);

        Ok(Self {
            issuer: Arc::new(issuer),
            cert_pem,
            cert_der,
        })
    }

    fn persist(&self, dir: &Path, cert_path: &Path, key_path: &Path) -> Result<(), ForwardError> {
        std::fs::create_dir_all(dir)
            .map_err(|e| ForwardError::CaInit(format!("create {}: {e}", dir.display())))?;

        atomic_write(cert_path, self.cert_pem.as_bytes())?;

        let key_pem = self.issuer.key().serialize_pem();
        write_key_0600(key_path, key_pem.as_bytes())?;

        Ok(())
    }

    pub fn cert_pem(&self) -> &str {
        &self.cert_pem
    }

    pub fn cert_der(&self) -> &rustls_pki_types::CertificateDer<'static> {
        &self.cert_der
    }

    pub fn issuer(&self) -> Arc<Issuer<'static, KeyPair>> {
        Arc::clone(&self.issuer)
    }
}

fn log_ca_loaded(dir: &Path) {
    tracing::info!(dir = %dir.display(), "loaded existing MITM CA");
}

fn log_ca_load_failed(e: &ForwardError) {
    tracing::warn!(
        error = %e,
        "existing CA files failed to parse; regenerating"
    );
}

fn pem_to_cert_der(
    cert_pem: &str,
) -> Result<rustls_pki_types::CertificateDer<'static>, ForwardError> {
    use rustls_pki_types::pem::PemObject;
    rustls_pki_types::CertificateDer::from_pem_slice(cert_pem.as_bytes())
        .map_err(|e| ForwardError::CaInit(format!("parse CA cert PEM: {e}")))
}

#[cfg(unix)]
fn write_key_0600(path: &Path, bytes: &[u8]) -> Result<(), ForwardError> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let tmp_path = tmp_path_for(path);
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp_path)
        .map_err(|e| ForwardError::CaInit(format!("open {} 0600: {e}", tmp_path.display())))?;
    f.write_all(bytes)
        .map_err(|e| ForwardError::CaInit(format!("write {}: {e}", tmp_path.display())))?;
    f.set_permissions(std::fs::Permissions::from_mode(0o600))
        .map_err(|e| ForwardError::CaInit(format!("chmod {} 0600: {e}", tmp_path.display())))?;
    drop(f);
    std::fs::rename(&tmp_path, path).map_err(|e| {
        ForwardError::CaInit(format!(
            "rename {} -> {}: {e}",
            tmp_path.display(),
            path.display()
        ))
    })?;
    Ok(())
}

#[cfg(not(unix))]
fn write_key_0600(path: &Path, bytes: &[u8]) -> Result<(), ForwardError> {
    atomic_write(path, bytes)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), ForwardError> {
    let tmp_path = tmp_path_for(path);
    std::fs::write(&tmp_path, bytes)
        .map_err(|e| ForwardError::CaInit(format!("write {}: {e}", tmp_path.display())))?;
    std::fs::rename(&tmp_path, path).map_err(|e| {
        ForwardError::CaInit(format!(
            "rename {} -> {}: {e}",
            tmp_path.display(),
            path.display()
        ))
    })?;
    Ok(())
}

fn tmp_path_for(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("ca-tmp");
    path.with_file_name(format!("{file_name}.tmp.{}", std::process::id()))
}

pub fn ca_paths(dir: &Path) -> (PathBuf, PathBuf) {
    (dir.join(CA_CERT_FILE), dir.join(CA_KEY_FILE))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_persist_reload_roundtrip() -> Result<(), ForwardError> {
        let dir = std::env::temp_dir().join(format!("pf-ca-test-{}", uuid_like()));
        let ca = Ca::load_or_generate(&dir)?;
        let first_pem = ca.cert_pem().to_string();

        let (cert_path, key_path) = ca_paths(&dir);
        assert!(cert_path.is_file(), "cert persisted");
        assert!(key_path.is_file(), "key persisted");

        let ca2 = Ca::load_or_generate(&dir)?;
        assert_eq!(
            first_pem,
            ca2.cert_pem(),
            "reload returns the same persisted CA, no re-mint"
        );

        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn key_file_is_owner_only() -> Result<(), ForwardError> {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("pf-ca-perm-{}", uuid_like()));
        let _ca = Ca::load_or_generate(&dir)?;
        let (_cert, key_path) = ca_paths(&dir);
        let mode = std::fs::metadata(&key_path)
            .map_err(ForwardError::Io)?
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "CA key must be 0600");
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn write_key_0600_forces_mode_over_existing_file() -> Result<(), ForwardError> {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("pf-ca-chmod-{}", uuid_like()));
        std::fs::create_dir_all(&dir).map_err(ForwardError::Io)?;
        let key_path = dir.join(CA_KEY_FILE);

        std::fs::write(&key_path, b"stale").map_err(ForwardError::Io)?;
        std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o644))
            .map_err(ForwardError::Io)?;
        assert_eq!(
            std::fs::metadata(&key_path)
                .map_err(ForwardError::Io)?
                .permissions()
                .mode()
                & 0o777,
            0o644,
            "precondition: pre-existing key is 0644"
        );

        write_key_0600(&key_path, b"fresh-key-bytes")?;
        let mode = std::fs::metadata(&key_path)
            .map_err(ForwardError::Io)?
            .permissions()
            .mode();
        assert_eq!(
            mode & 0o777,
            0o600,
            "write_key_0600 must force 0600 even over a pre-existing 0644 file"
        );

        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn issuer_can_sign_a_leaf() -> Result<(), ForwardError> {
        let dir = std::env::temp_dir().join(format!("pf-ca-sign-{}", uuid_like()));
        let ca = Ca::load_or_generate(&dir)?;
        let leaf_key = KeyPair::generate().map_err(|e| ForwardError::CaInit(e.to_string()))?;
        let mut params = CertificateParams::new(vec!["api.anthropic.com".to_string()])
            .map_err(|e| ForwardError::CaInit(e.to_string()))?;
        params
            .distinguished_name
            .push(DnType::CommonName, "api.anthropic.com");
        let leaf =
            params
                .signed_by(&leaf_key, &ca.issuer())
                .map_err(|e| ForwardError::CertMint {
                    host: "api.anthropic.com".to_string(),
                    reason: e.to_string(),
                })?;
        assert!(!leaf.der().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    fn uuid_like() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        format!("{nanos}-{:?}", std::thread::current().id()).replace(['(', ')', ' '], "")
    }
}
