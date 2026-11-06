#[derive(Debug, thiserror::Error)]
pub enum ForwardError {
    #[error("failed to bind forward-proxy listener on {addr}: {source}")]
    Bind {
        addr: String,
        #[source]
        source: std::io::Error,
    },

    #[error("CA initialisation failed: {0}")]
    CaInit(String),

    #[error("certificate minting failed for {host}: {reason}")]
    CertMint { host: String, reason: String },

    #[error("TLS server config build failed: {0}")]
    TlsConfig(String),

    #[error("client TLS handshake failed: {0}")]
    TlsAccept(String),

    #[error("upstream TLS client build failed: {0}")]
    TlsConnect(String),

    #[error("bad CONNECT target: {0}")]
    BadConnectTarget(String),

    #[error("upstream connect failed: {0}")]
    UpstreamConnect(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
