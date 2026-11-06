pub(super) fn replace_db_name(original: &str, new_db: &str) -> anyhow::Result<String> {
    let (prefix, tail) = split_on_host_separator(original)?;
    let (host_port, db_and_query) = match tail.split_once('/') {
        Some((h, rest)) => (h, rest),
        None => (tail, ""),
    };
    let query = db_and_query
        .split_once('?')
        .map(|(_db, q)| format!("?{q}"))
        .unwrap_or_default();
    Ok(format!("{prefix}{host_port}/{new_db}{query}"))
}

pub(super) fn extract_db_name(url: &str) -> anyhow::Result<String> {
    let (_prefix, tail) = split_on_host_separator(url)?;
    let (_host_port, db_and_query) = tail.split_once('/').ok_or_else(|| {
        anyhow::anyhow!(
            "MIKE_TEST_ADMIN_URL must include an explicit database component (e.g. .../postgres)"
        )
    })?;
    let db = match db_and_query.split_once('?') {
        Some((d, _)) => d,
        None => db_and_query,
    };
    if db.is_empty() {
        anyhow::bail!(
            "MIKE_TEST_ADMIN_URL has an empty database component — append a name (e.g. /postgres)"
        );
    }
    Ok(db.to_string())
}

pub(super) fn extract_host_port(url: &str) -> anyhow::Result<String> {
    let (_prefix, tail) = split_on_host_separator(url)?;
    let host_port = tail.split_once('/').map_or(tail, |(h, _)| h);
    Ok(host_port.to_string())
}

fn split_on_host_separator(url: &str) -> anyhow::Result<(&str, &str)> {
    let scheme_end = url
        .find("://")
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL missing scheme separator `://`"))?
        + 3;
    let rest = &url[scheme_end..];
    match rest.find('@') {
        Some(at_idx) => Ok((&url[..scheme_end + at_idx + 1], &rest[at_idx + 1..])),
        None => Ok((&url[..scheme_end], rest)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_db_name_handles_basic_url() -> anyhow::Result<()> {
        assert_eq!(
            extract_db_name("postgres://proxy:proxy@localhost:6666/postgres")?,
            "postgres"
        );
        Ok(())
    }

    #[test]
    fn extract_db_name_strips_query_string() -> anyhow::Result<()> {
        assert_eq!(
            extract_db_name("postgres://proxy@localhost/claude_proxy?sslmode=disable")?,
            "claude_proxy"
        );
        Ok(())
    }

    #[test]
    fn extract_db_name_rejects_missing_component() -> anyhow::Result<()> {
        let err = match extract_db_name("postgres://proxy:proxy@localhost:6666") {
            Ok(db) => anyhow::bail!("expected error for missing db component, got db={db}"),
            Err(e) => e,
        };
        assert!(
            err.to_string().contains("explicit database component"),
            "got: {err}"
        );
        Ok(())
    }

    #[test]
    fn replace_db_name_preserves_creds_and_query() -> anyhow::Result<()> {
        let out = replace_db_name(
            "postgres://proxy:pw@localhost:6666/postgres?sslmode=disable",
            "claude_proxy_test_xyz",
        )?;
        assert_eq!(
            out,
            "postgres://proxy:pw@localhost:6666/claude_proxy_test_xyz?sslmode=disable"
        );
        Ok(())
    }

    #[test]
    fn replace_db_name_without_credentials() -> anyhow::Result<()> {
        let out = replace_db_name("postgres://localhost/postgres", "claude_proxy_test_foo")?;
        assert_eq!(out, "postgres://localhost/claude_proxy_test_foo");
        Ok(())
    }
}
