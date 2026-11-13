use std::sync::Arc;

use dashmap::DashMap;
use rcgen::{CertificateParams, DnType, KeyPair};
use rustls::server::{ClientHello, ResolvesServerCert};
use rustls::sign::CertifiedKey;

use crate::ca::Ca;

#[derive(Debug)]
pub struct LeafResolver {
    issuer: Arc<rcgen::Issuer<'static, KeyPair>>,
    ca_der: rustls_pki_types::CertificateDer<'static>,
    intercept_host: String,
    cache: DashMap<String, Arc<CertifiedKey>>,
}

impl LeafResolver {
    pub fn new(ca: &Ca, intercept_host: String) -> Self {
        Self {
            issuer: ca.issuer(),
            ca_der: ca.cert_der().clone(),
            intercept_host: intercept_host.to_ascii_lowercase(),
            cache: DashMap::new(),
        }
    }

    fn certified_key_for(&self, host: &str) -> Option<Arc<CertifiedKey>> {
        let host = host.to_ascii_lowercase();
        if host != self.intercept_host {
            return None;
        }
        if let Some(existing) = self.cache.get(&host) {
            return Some(Arc::clone(existing.value()));
        }

        let key = match self.mint(&host) {
            Ok(k) => Arc::new(k),
            Err(e) => {
                tracing::warn!(error = %e, host, "failed to mint leaf certificate");
                return None;
            }
        };
        self.cache.insert(host, Arc::clone(&key));
        Some(key)
    }

    fn mint(&self, host: &str) -> Result<CertifiedKey, String> {
        let leaf_key = KeyPair::generate().map_err(|e| format!("generate leaf key: {e}"))?;

        let mut params = CertificateParams::new(vec![host.to_string()])
            .map_err(|e| format!("leaf params for {host}: {e}"))?;
        params.distinguished_name.push(DnType::CommonName, host);

        let leaf = params
            .signed_by(&leaf_key, &self.issuer)
            .map_err(|e| format!("sign leaf for {host}: {e}"))?;

        let leaf_der: rustls_pki_types::CertificateDer<'static> = leaf.der().clone();
        let key_der = rustls_pki_types::PrivateKeyDer::Pkcs8(
            rustls_pki_types::PrivatePkcs8KeyDer::from(leaf_key.serialize_der()),
        );

        let signing_key = rustls::crypto::ring::sign::any_supported_type(&key_der)
            .map_err(|e| format!("adopt leaf signing key: {e}"))?;

        let chain = vec![leaf_der, self.ca_der.clone()];
        Ok(CertifiedKey::new(chain, signing_key))
    }
}

impl ResolvesServerCert for LeafResolver {
    fn resolve(&self, client_hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        let host = client_hello.server_name()?;
        self.certified_key_for(host)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ForwardError;

    fn test_ca() -> Result<(Ca, tempfile::TempDir), ForwardError> {
        let dir = tempfile::tempdir().map_err(ForwardError::Io)?;
        let ca = Ca::load_or_generate(&dir.path().join("ca"))?;
        Ok((ca, dir))
    }

    #[test]
    fn mints_for_intercept_host() -> Result<(), ForwardError> {
        let (ca, dir) = test_ca()?;
        let resolver = LeafResolver::new(&ca, "api.anthropic.com".to_string());
        let key = resolver.certified_key_for("api.anthropic.com");
        assert!(key.is_some(), "intercept host yields a CertifiedKey");
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn mints_for_mixed_case_sni() -> Result<(), ForwardError> {
        let (ca, dir) = test_ca()?;
        let resolver = LeafResolver::new(&ca, "api.anthropic.com".to_string());
        let key = resolver.certified_key_for("API.Anthropic.Com");
        assert!(key.is_some(), "mixed-case SNI yields a CertifiedKey");
        let canonical = resolver
            .certified_key_for("api.anthropic.com")
            .ok_or_else(|| ForwardError::CaInit("canonical mint returned None".to_string()))?;
        let mixed =
            key.ok_or_else(|| ForwardError::CaInit("mixed mint returned None".to_string()))?;
        assert!(
            Arc::ptr_eq(&mixed, &canonical),
            "mixed-case and canonical SNI resolve to the same cached Arc"
        );
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn declines_other_host() -> Result<(), ForwardError> {
        let (ca, dir) = test_ca()?;
        let resolver = LeafResolver::new(&ca, "api.anthropic.com".to_string());
        assert!(
            resolver.certified_key_for("example.com").is_none(),
            "non-intercept host is declined"
        );
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn cache_returns_same_arc() -> Result<(), ForwardError> {
        let (ca, dir) = test_ca()?;
        let resolver = LeafResolver::new(&ca, "api.anthropic.com".to_string());
        let a = resolver
            .certified_key_for("api.anthropic.com")
            .ok_or_else(|| ForwardError::CaInit("first mint returned None".to_string()))?;
        let b = resolver
            .certified_key_for("api.anthropic.com")
            .ok_or_else(|| ForwardError::CaInit("second fetch returned None".to_string()))?;
        assert!(Arc::ptr_eq(&a, &b), "cache returns the same Arc");
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }
}
