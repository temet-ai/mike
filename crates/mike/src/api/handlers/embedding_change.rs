use axum::http::{HeaderMap, StatusCode};

pub(super) fn validate_disable(
    key: &str,
    value: &serde_json::Value,
    headers: &HeaderMap,
) -> Result<(), (StatusCode, &'static str)> {
    if key != "embedding.enabled" || value.as_bool() != Some(false) {
        return Ok(());
    }
    let present = |name: &str| {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| !value.trim().is_empty())
    };
    if !present("x-change-reason") || !(present("user-agent") || present("origin")) {
        return Err((
            StatusCode::BAD_REQUEST,
            "Disabling embedding pauses semantic ingestion for every project. Supply X-Change-Reason and an identifiable User-Agent or Origin; the reason is recorded in settings history.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anonymous_disable_is_rejected_but_enable_is_allowed() {
        assert!(
            validate_disable(
                "embedding.enabled",
                &serde_json::json!(false),
                &HeaderMap::new()
            )
            .is_err()
        );
        assert!(
            validate_disable(
                "embedding.enabled",
                &serde_json::json!(true),
                &HeaderMap::new()
            )
            .is_ok()
        );
    }

    #[test]
    fn identified_disable_requires_a_reason() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "user-agent",
            axum::http::HeaderValue::from_static("mike-test"),
        );
        assert!(
            validate_disable("embedding.enabled", &serde_json::json!(false), &headers).is_err()
        );
        headers.insert(
            "x-change-reason",
            axum::http::HeaderValue::from_static("isolated test"),
        );
        assert!(validate_disable("embedding.enabled", &serde_json::json!(false), &headers).is_ok());
    }
}
