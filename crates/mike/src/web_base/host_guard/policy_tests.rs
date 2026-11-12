use axum::http::{HeaderMap, HeaderValue, Uri, Version, header};

use super::LocalAccessPolicy;

fn check(bind: &str, host: Option<&str>, origin: Option<&str>) -> anyhow::Result<bool> {
    let mut headers = HeaderMap::new();
    if let Some(host) = host {
        headers.insert(header::HOST, host.parse()?);
    }
    if let Some(origin) = origin {
        headers.insert(header::ORIGIN, origin.parse()?);
    }
    Ok(LocalAccessPolicy::new(bind, &[], &[])
        .check(
            &headers,
            &Uri::from_static("/api/requests"),
            Version::HTTP_11,
        )
        .is_ok())
}

#[test]
fn listener_authorities_require_exact_ports() -> anyhow::Result<()> {
    for (bind, host) in [
        ("127.0.0.1:9989", "localhost:9989"),
        ("127.0.0.1:9990", "127.0.0.1:9990"),
        ("[::1]:18080", "[0:0:0:0:0:0:0:1]:18080"),
        ("127.0.0.1:80", "localhost"),
        ("[::1]:80", "[::1]:80"),
        ("127.0.0.2:18080", "127.0.0.2:18080"),
    ] {
        assert!(check(bind, Some(host), None)?, "{bind}, {host}");
    }
    for host in [
        "localhost",
        "localhost:9990",
        "localhost:80",
        "127.0.0.2:9989",
        "[::1]:443",
    ] {
        assert!(!check("127.0.0.1:9989", Some(host), None)?, "{host}");
    }
    Ok(())
}

#[test]
fn malformed_and_confusable_hosts_are_rejected() -> anyhow::Result<()> {
    for host in [
        "",
        "localhost:",
        "localhost:65536",
        "localhost:foo",
        "localhost:9989/",
        "localhost:9989?x",
        "user@localhost:9989",
        "localhost.evil:9989",
        "localhost.:9989",
        "127.1:9989",
        "2130706433:9989",
        "0x7f000001:9989",
        "localhost:9989,localhost:9989",
        "localhost:9989 localhost:9989",
        "[::1]:9989:80",
    ] {
        assert!(!check("127.0.0.1:9989", Some(host), None)?, "{host}");
    }
    assert!(!check("127.0.0.1:9989", None, None)?);
    Ok(())
}

#[test]
fn origins_compare_scheme_host_and_effective_port() -> anyhow::Result<()> {
    for (bind, host, origin) in [
        ("127.0.0.1:9989", "localhost:9989", "http://localhost:9989"),
        ("127.0.0.1:9990", "localhost:9990", "http://localhost:9990"),
        (
            "[::1]:18080",
            "[::1]:18080",
            "http://[0:0:0:0:0:0:0:1]:18080",
        ),
        ("127.0.0.1:80", "localhost", "http://localhost:80"),
        ("127.0.0.1:80", "localhost:80", "http://localhost"),
        ("127.0.0.1:9989", "127.0.0.1:9989", "http://localhost:9990"),
    ] {
        assert!(check(bind, Some(host), Some(origin))?, "{origin}");
    }
    for origin in [
        "https://localhost:9989",
        "https://localhost",
        "http://localhost",
        "http://localhost:9991",
    ] {
        assert!(
            !check("127.0.0.1:9989", Some("localhost:9989"), Some(origin))?,
            "{origin}"
        );
    }
    Ok(())
}

#[test]
fn invalid_and_confusable_origins_are_rejected() -> anyhow::Result<()> {
    for origin in [
        "null",
        "NULL",
        "",
        "http://localhost:9989/",
        "http://localhost:9989/path",
        "http://localhost:9989?query",
        "http://localhost:9989#fragment",
        "http://user@localhost:9989",
        "http://localhost.evil:9989",
        "http://localhost.:9989",
        "http://127.1:9989",
        "http://2130706433:9989",
        "http://0x7f000001:9989",
        "http://localhost:9989 http://evil.example",
        "http://localhost:9989,http://localhost:9989",
        "http://localhost:9989\t",
        " http://localhost:9989",
        "ftp://localhost:9989",
        "file://localhost:9989",
        "http:\\\\localhost:9989",
    ] {
        assert!(
            !check("127.0.0.1:9989", Some("localhost:9989"), Some(origin))?,
            "{origin}"
        );
    }
    Ok(())
}

#[test]
fn duplicate_and_non_ascii_security_headers_fail_closed() -> anyhow::Result<()> {
    let policy = LocalAccessPolicy::new("127.0.0.1:9989", &[], &[]);
    let uri = Uri::from_static("/api/ws");
    for name in [header::HOST, header::ORIGIN] {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("localhost:9989"));
        let value = if name == header::HOST {
            "localhost:9989"
        } else {
            "http://localhost:9989"
        };
        headers.insert(name.clone(), value.parse()?);
        headers.append(name.clone(), value.parse()?);
        assert!(
            policy.check(&headers, &uri, Version::HTTP_11).is_err(),
            "{name}"
        );
        headers.remove(&name);
        headers.insert(name.clone(), HeaderValue::from_bytes(b"\xff")?);
        assert!(
            policy.check(&headers, &uri, Version::HTTP_11).is_err(),
            "{name}"
        );
    }
    Ok(())
}

#[test]
fn configured_origins_do_not_authorize_request_hosts() -> anyhow::Result<()> {
    let origins = vec!["https://ui.example:443".to_owned(), "null".to_owned()];
    let policy = LocalAccessPolicy::new("127.0.0.1:9989", &[], &origins);
    let mut headers = HeaderMap::new();
    headers.insert(header::HOST, "localhost:9989".parse()?);
    headers.insert(header::ORIGIN, "https://ui.example".parse()?);
    let uri = Uri::from_static("/api/requests");
    assert!(policy.check(&headers, &uri, Version::HTTP_11).is_ok());
    headers.insert(header::HOST, "ui.example:9989".parse()?);
    assert!(policy.check(&headers, &uri, Version::HTTP_11).is_err());
    Ok(())
}

#[test]
fn explicit_host_aliases_remain_exact() -> anyhow::Result<()> {
    let hosts = vec![
        "inspect.example".to_owned(),
        "tunnel.example:18080".to_owned(),
    ];
    let policy = LocalAccessPolicy::new("127.0.0.1:9989", &hosts, &[]);
    for (host, accepted) in [
        ("inspect.example:9989", true),
        ("tunnel.example:18080", true),
        ("inspect.example:18080", false),
        ("inspect.example.evil:9989", false),
    ] {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, host.parse()?);
        assert_eq!(
            policy
                .check(
                    &headers,
                    &Uri::from_static("/api/requests"),
                    Version::HTTP_11
                )
                .is_ok(),
            accepted,
            "{host}"
        );
    }
    Ok(())
}

#[test]
fn request_authority_is_validated_without_trusting_forwarded_headers() -> anyhow::Result<()> {
    let policy = LocalAccessPolicy::new("127.0.0.1:9989", &[], &[]);
    let mut headers = HeaderMap::new();
    let uri: Uri = "http://localhost:9989/api/requests".parse()?;
    assert!(policy.check(&headers, &uri, Version::HTTP_2).is_ok());
    assert!(policy.check(&headers, &uri, Version::HTTP_11).is_err());
    headers.insert(header::HOST, "localhost:9990".parse()?);
    assert!(policy.check(&headers, &uri, Version::HTTP_2).is_err());
    headers.insert(header::HOST, "localhost:9989".parse()?);
    headers.insert(header::ORIGIN, "https://localhost:9989".parse()?);
    headers.insert("x-forwarded-proto", "https".parse()?);
    assert!(policy.check(&headers, &uri, Version::HTTP_11).is_err());
    Ok(())
}

#[test]
fn originless_fetch_metadata_blocks_browser_cross_site_gets() -> anyhow::Result<()> {
    let policy = LocalAccessPolicy::new("127.0.0.1:9989", &[], &[]);
    let mut headers = HeaderMap::new();
    headers.insert(header::HOST, "localhost:9989".parse()?);
    let uri = Uri::from_static("/api/requests");
    for (site, accepted) in [
        ("same-origin", true),
        ("none", true),
        ("same-site", false),
        ("cross-site", false),
        ("invalid", false),
    ] {
        headers.insert("sec-fetch-site", site.parse()?);
        assert_eq!(
            policy.check(&headers, &uri, Version::HTTP_11).is_ok(),
            accepted,
            "{site}"
        );
    }
    headers.remove("sec-fetch-site");
    assert!(policy.check(&headers, &uri, Version::HTTP_11).is_ok());
    Ok(())
}

#[test]
fn configured_origins_are_canonicalized_without_permissive_url_repair() -> anyhow::Result<()> {
    let origin = super::super::authority::Origin::parse("HTTPS://UI.EXAMPLE:443")
        .ok_or_else(|| anyhow::anyhow!("valid origin"))?;
    assert_eq!(origin.serialize(), "https://ui.example");
    assert!(super::origin_is_allowed(
        "https://ui.example",
        None,
        &[origin.serialize()]
    ));
    assert!(!super::origin_is_allowed(
        "null",
        Some("localhost:9989"),
        &["null".to_owned()]
    ));
    assert!(!super::origin_is_allowed(
        "http://localhost:9990",
        None,
        &["invalid".to_owned()]
    ));
    Ok(())
}
