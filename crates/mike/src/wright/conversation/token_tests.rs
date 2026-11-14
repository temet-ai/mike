use axum::http::{HeaderMap, HeaderValue, header};

use super::fakes_tests::{TOKEN, bearer, operator_token};
use super::token::{OperatorToken, authorize, ensure, load};

#[cfg(unix)]
#[test]
fn ensure_without_a_file_writes_an_owner_only_hex_token_once() -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("state").join("operator-token");
    let first = ensure(&path)?;
    assert_eq!(first.expose().len(), 64);
    assert!(first.expose().chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(
        std::fs::metadata(&path)?.permissions().mode() & 0o777,
        0o600
    );
    let second = ensure(&path)?;
    assert_eq!(first.expose(), second.expose());
    let loaded = load(&path)?.ok_or_else(|| anyhow::anyhow!("the token file is empty"))?;
    assert_eq!(loaded.expose(), first.expose());
    Ok(())
}

#[test]
fn generated_tokens_differ() -> anyhow::Result<()> {
    assert_ne!(
        OperatorToken::generate()?.expose(),
        OperatorToken::generate()?.expose()
    );
    Ok(())
}

#[test]
fn matches_only_the_exact_token() -> anyhow::Result<()> {
    let token = operator_token()?;
    assert!(token.matches(TOKEN));
    assert!(!token.matches(&TOKEN[..63]));
    assert!(!token.matches(&format!("{TOKEN}0")));
    assert!(!token.matches(""));
    Ok(())
}

#[test]
fn authorize_without_a_bearer_or_with_identity_headers_only_is_refused() -> anyhow::Result<()> {
    let token = operator_token()?;
    assert!(authorize(&HeaderMap::new(), Some(&token)).is_err());
    let mut claims = HeaderMap::new();
    claims.insert(header::HOST, HeaderValue::from_static("localhost:9990"));
    claims.insert(
        header::ORIGIN,
        HeaderValue::from_static("http://localhost:9990"),
    );
    claims.insert("x-sender", HeaderValue::from_static("operator"));
    assert!(authorize(&claims, Some(&token)).is_err());
    assert!(authorize(&bearer("not-the-token")?, Some(&token)).is_err());
    assert!(authorize(&bearer(TOKEN)?, None).is_err());
    Ok(())
}

#[test]
fn authorize_with_the_operator_bearer_passes_in_any_scheme_case() -> anyhow::Result<()> {
    let token = operator_token()?;
    assert!(authorize(&bearer(TOKEN)?, Some(&token)).is_ok());
    let mut lower = HeaderMap::new();
    lower.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("bearer {TOKEN}"))?,
    );
    assert!(authorize(&lower, Some(&token)).is_ok());
    let mut basic = HeaderMap::new();
    basic.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Basic {TOKEN}"))?,
    );
    assert!(authorize(&basic, Some(&token)).is_err());
    Ok(())
}
