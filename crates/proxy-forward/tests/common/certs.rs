use anyhow::{Context, Result};
use rustls_pki_types::{CertificateDer, PrivateKeyDer};

pub struct TestCert {
    pub ca_der: CertificateDer<'static>,
    pub ca_pem: String,
    pub leaf_chain: Vec<CertificateDer<'static>>,
    pub leaf_key: PrivateKeyDer<'static>,
}

pub fn mint_cert(host: &str) -> Result<TestCert> {
    use rcgen::{
        BasicConstraints, CertificateParams, DnType, IsCa, Issuer, KeyPair, KeyUsagePurpose,
    };

    let ca_key = KeyPair::generate().context("ca key")?;
    let mut ca_params = CertificateParams::default();
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
    ca_params
        .distinguished_name
        .push(DnType::CommonName, "proxy-forward test CA");
    ca_params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    let ca_cert = ca_params.self_signed(&ca_key).context("ca self-sign")?;
    let ca_pem = ca_cert.pem();
    let ca_der: CertificateDer<'static> = ca_cert.der().clone();

    let issuer = Issuer::new(ca_params, ca_key);

    let leaf_key = KeyPair::generate().context("leaf key")?;
    let mut leaf_params = CertificateParams::new(vec![host.to_string()]).context("leaf params")?;
    leaf_params
        .distinguished_name
        .push(DnType::CommonName, host);
    let leaf = leaf_params
        .signed_by(&leaf_key, &issuer)
        .context("sign leaf")?;
    let leaf_der: CertificateDer<'static> = leaf.der().clone();
    let key_der = PrivateKeyDer::Pkcs8(leaf_key.serialize_der().into());

    Ok(TestCert {
        ca_der: ca_der.clone(),
        ca_pem,
        leaf_chain: vec![leaf_der, ca_der],
        leaf_key: key_der,
    })
}

pub fn install_crypto() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}
