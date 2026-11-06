use std::sync::Arc;

use bytes::Bytes;
use http_body_util::combinators::BoxBody;
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;

use crate::error::ForwardError;

pub type ForwardBody = BoxBody<Bytes, hyper::Error>;

pub type RoutedClient = Client<HttpConnector, ForwardBody>;

pub type DirectClient = Client<HttpsConnector<HttpConnector>, ForwardBody>;

#[derive(Clone)]
pub struct UpstreamClients {
    pub routed: RoutedClient,
    pub direct: Arc<DirectClient>,
}

impl UpstreamClients {
    pub fn build(
        extra_direct_roots: &[rustls_pki_types::CertificateDer<'static>],
        pinned_sni: Option<&str>,
    ) -> Result<Self, ForwardError> {
        let routed: RoutedClient = Client::builder(TokioExecutor::new()).build_http();

        let native = rustls_native_certs::load_native_certs();
        for err in &native.errors {
            tracing::warn!(error = %err, "error while loading a native root cert");
        }
        let roots = assemble_direct_roots(native.certs, extra_direct_roots)?;

        let tls = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();

        let stage1 = hyper_rustls::HttpsConnectorBuilder::new()
            .with_tls_config(tls)
            .https_only();

        let stage1 = match pinned_sni {
            Some(name) => {
                let server_name = rustls_pki_types::ServerName::try_from(name.to_string())
                    .map_err(|e| ForwardError::TlsConnect(format!("pin SNI {name:?}: {e}")))?;
                stage1.with_server_name_resolver(hyper_rustls::FixedServerNameResolver::new(
                    server_name,
                ))
            }
            None => stage1,
        };

        let https = stage1.enable_all_versions().build();

        let direct: DirectClient = Client::builder(TokioExecutor::new()).build(https);

        Ok(Self {
            routed,
            direct: Arc::new(direct),
        })
    }
}

fn assemble_direct_roots(
    native_certs: Vec<rustls_pki_types::CertificateDer<'static>>,
    extra_direct_roots: &[rustls_pki_types::CertificateDer<'static>],
) -> Result<rustls::RootCertStore, ForwardError> {
    let mut roots = rustls::RootCertStore::empty();
    for cert in native_certs {
        let _ = roots.add(cert);
    }
    for extra in extra_direct_roots {
        roots
            .add(extra.clone())
            .map_err(|e| ForwardError::TlsConnect(format!("add extra root: {e}")))?;
    }
    if roots.is_empty() {
        return Err(ForwardError::TlsConnect(
            "direct-leg trust store is empty (no native roots loaded and no extra CA \
             supplied); the direct/failover leg cannot verify TLS — refusing to start"
                .to_string(),
        ));
    }
    Ok(roots)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_native_and_no_extras_fails_closed() {
        let err = match assemble_direct_roots(Vec::new(), &[]) {
            Err(e) => e,
            Ok(_) => panic!("empty trust store must fail closed"),
        };
        match err {
            ForwardError::TlsConnect(msg) => {
                assert!(
                    msg.contains("empty"),
                    "message names the empty store: {msg}"
                );
            }
            other => panic!("expected TlsConnect, got {other:?}"),
        }
    }

    #[test]
    fn extra_root_makes_store_usable() -> Result<(), ForwardError> {
        let key = rcgen::KeyPair::generate()
            .map_err(|e| ForwardError::TlsConnect(format!("gen key: {e}")))?;
        let mut params = rcgen::CertificateParams::default();
        params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        let cert = params
            .self_signed(&key)
            .map_err(|e| ForwardError::TlsConnect(format!("self-sign: {e}")))?;
        let der: rustls_pki_types::CertificateDer<'static> = cert.der().clone();

        let roots = assemble_direct_roots(Vec::new(), std::slice::from_ref(&der))?;
        assert!(!roots.is_empty(), "extra root makes the store usable");
        Ok(())
    }
}
