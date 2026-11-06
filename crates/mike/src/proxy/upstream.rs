pub(crate) fn build_upstream_url(upstream_url: &str, path: &str, query: Option<&str>) -> String {
    let base = upstream_url.trim_end_matches('/');
    match query {
        Some(q) => format!("{base}{path}?{q}"),
        None => format!("{base}{path}"),
    }
}

const MAX_CONNECT_RETRIES: u32 = 3;
const RETRY_BASE_DELAY_MS: u64 = 100;

pub(crate) async fn send_with_retry(
    client: &reqwest::Client,
    request: reqwest::Request,
) -> Result<reqwest::Response, reqwest::Error> {
    let mut attempt: u32 = 0;
    loop {
        let attempt_req = match request.try_clone() {
            Some(req) => req,
            None => return client.execute(request).await,
        };

        match client.execute(attempt_req).await {
            Ok(resp) => return Ok(resp),
            Err(e) if e.is_connect() && attempt < MAX_CONNECT_RETRIES => {
                let delay_ms = RETRY_BASE_DELAY_MS << attempt;
                tracing::warn!(
                    attempt = attempt + 1,
                    max_retries = MAX_CONNECT_RETRIES,
                    delay_ms,
                    error = %e,
                    "upstream connect failed — retrying with exponential backoff",
                );
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    }
}

pub(crate) fn extract_host(url: &str) -> String {
    let host = url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or("");
    if host.is_empty() {
        "api.anthropic.com".to_string()
    } else {
        host.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_upstream_url_no_query() {
        let url = build_upstream_url("https://api.anthropic.com", "/v1/messages", None);
        assert_eq!(url, "https://api.anthropic.com/v1/messages");
    }

    #[test]
    fn test_build_upstream_url_with_query() {
        let url = build_upstream_url(
            "https://api.anthropic.com",
            "/v1/messages",
            Some("beta=true"),
        );
        assert_eq!(url, "https://api.anthropic.com/v1/messages?beta=true");
    }

    #[test]
    fn test_build_upstream_url_trailing_slash() {
        let url = build_upstream_url("https://api.anthropic.com/", "/v1/messages", None);
        assert_eq!(url, "https://api.anthropic.com/v1/messages");
    }

    #[test]
    fn test_build_upstream_url_no_path() {
        let url = build_upstream_url("https://api.anthropic.com", "/", None);
        assert_eq!(url, "https://api.anthropic.com/");
    }

    #[test]
    fn extract_host_https() {
        assert_eq!(
            extract_host("https://api.anthropic.com"),
            "api.anthropic.com"
        );
    }

    #[test]
    fn extract_host_http() {
        assert_eq!(extract_host("http://localhost:8080"), "localhost:8080");
    }

    #[test]
    fn extract_host_with_path() {
        assert_eq!(
            extract_host("https://example.com/v1/messages"),
            "example.com"
        );
    }

    #[test]
    fn extract_host_empty_falls_back_to_anthropic() {
        assert_eq!(extract_host(""), "api.anthropic.com");
    }
}
