use axum::http::{HeaderMap, header};

use crate::config::ProxyConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpaOrigin {
    origin: String,
    request_id: Option<String>,
}

impl SpaOrigin {
    pub fn checked(
        origin: Option<&str>,
        host: Option<&str>,
        request_id: Option<String>,
        configured: &[String],
    ) -> Option<Self> {
        let origin = origin?;
        crate::web_base::host_guard::origin_is_allowed(origin, host, configured).then(|| Self {
            origin: origin.to_owned(),
            request_id,
        })
    }

    pub fn origin(&self) -> &str {
        &self.origin
    }

    pub fn request_id(&self) -> Option<&str> {
        self.request_id.as_deref()
    }
}

fn header_text(headers: &HeaderMap, name: impl header::AsHeaderName) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
}

pub fn spa_origin(headers: &HeaderMap, config: &ProxyConfig) -> Option<SpaOrigin> {
    SpaOrigin::checked(
        header_text(headers, header::ORIGIN).as_deref(),
        header_text(headers, header::HOST).as_deref(),
        header_text(headers, "x-request-id"),
        &config.spa_origins,
    )
}

#[cfg(test)]
mod tests {
    use super::SpaOrigin;

    #[test]
    fn no_origin_and_null_origin_are_refused() {
        assert!(SpaOrigin::checked(None, Some("127.0.0.1:9989"), None, &[]).is_none());
        assert!(SpaOrigin::checked(Some("null"), Some("127.0.0.1:9989"), None, &[]).is_none());
    }

    #[test]
    fn a_configured_spa_origin_passes() {
        let configured = vec!["http://127.0.0.1:9990".to_owned()];
        let origin = SpaOrigin::checked(
            Some("http://127.0.0.1:9990"),
            Some("127.0.0.1:9989"),
            Some("req-1".to_owned()),
            &configured,
        );
        assert_eq!(
            origin.as_ref().map(SpaOrigin::origin),
            Some("http://127.0.0.1:9990")
        );
        assert_eq!(
            origin.as_ref().and_then(SpaOrigin::request_id),
            Some("req-1")
        );
    }
}
