pub(crate) fn redact_database_url(value: &str) -> String {
    let Ok(mut url) = url::Url::parse(value) else {
        return "<redacted>".to_owned();
    };
    if url.password().is_some() {
        let _ = url.set_password(Some("***"));
    }
    url.set_query(None);
    url.set_fragment(None);
    url.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_database_url_masks_password_segment() {
        let redacted = redact_database_url("postgres://proxy:secret@localhost:6666/claude_proxy");
        assert_eq!(
            redacted, "postgres://proxy:***@localhost:6666/claude_proxy",
            "password must be replaced with `***`"
        );
    }

    #[test]
    fn redact_database_url_leaves_hostname_intact_when_no_password() {
        let redacted = redact_database_url("postgres://proxy@localhost/db");
        assert_eq!(redacted, "postgres://proxy@localhost/db");
    }

    #[test]
    fn redact_database_url_hides_unparseable_values() {
        assert_eq!(redact_database_url("not a url"), "<redacted>");
        assert_eq!(
            redact_database_url("postgres://localhost/db"),
            "postgres://localhost/db"
        );
    }
    #[test]
    fn redact_database_url_removes_query_credentials_and_fragments() {
        let result =
            redact_database_url("postgres://u:p@localhost/db?password=planted-secret#other-secret");
        assert_eq!(result, "postgres://u:***@localhost/db");
    }
}
