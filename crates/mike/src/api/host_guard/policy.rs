use axum::http::{HeaderMap, HeaderName, Uri, Version, header};

use super::authority::{Authority, Origin};

const DEFAULT_ORIGINS: [&str; 2] = ["http://127.0.0.1:9990", "http://localhost:9990"];

#[derive(Clone, Debug)]
pub(crate) struct LocalAccessPolicy {
    pub(super) authorities: Vec<Authority>,
    origins: Vec<Origin>,
    pub(super) any_port: bool,
}

pub(super) fn configured_origins(configured: &[String]) -> impl Iterator<Item = &str> {
    let fallback = configured
        .is_empty()
        .then_some(DEFAULT_ORIGINS)
        .into_iter()
        .flatten();
    configured.iter().map(String::as_str).chain(fallback)
}

pub(super) fn origin_is_allowed(origin: &str, host: Option<&str>, configured: &[String]) -> bool {
    let Some(origin) = Origin::parse(origin) else {
        return false;
    };
    configured_origins(configured)
        .filter_map(Origin::parse)
        .any(|allowed| allowed == origin)
        || host
            .and_then(|host| Authority::parse(host, 80))
            .is_some_and(|host| origin.same_http_authority(&host))
}

impl LocalAccessPolicy {
    pub(crate) fn new(bind: &str, allowed_hosts: &[String], origins: &[String]) -> Self {
        let mut authorities = Vec::new();
        if let Some(bound) = Authority::parse(bind, 80) {
            authorities.extend(local_aliases(bound.port));
            authorities.extend(
                allowed_hosts
                    .iter()
                    .filter_map(|raw| Authority::parse(raw, bound.port)),
            );
            if !matches!(bound.host.as_str(), "0.0.0.0" | "[::]") {
                authorities.push(bound);
            }
        }
        Self {
            authorities,
            origins: configured_origins(origins)
                .filter_map(Origin::parse)
                .collect(),
            any_port: false,
        }
    }

    fn allows_host(&self, host: &Authority) -> bool {
        if self.any_port {
            self.authorities.iter().any(|known| known.host == host.host)
        } else {
            self.authorities.contains(host)
        }
    }

    pub(crate) fn check(
        &self,
        headers: &HeaderMap,
        uri: &Uri,
        version: Version,
    ) -> Result<(), &'static str> {
        let host = request_authority(headers, uri, version)?;
        if !self.allows_host(&host) {
            return Err("host not allowed");
        }
        let Some(origin) = single_header(headers, header::ORIGIN)? else {
            return check_originless_browser_request(headers);
        };
        let origin = Origin::parse(origin).ok_or("invalid origin")?;
        if self.origins.contains(&origin) || origin.same_http_authority(&host) {
            Ok(())
        } else {
            Err("origin not allowed")
        }
    }
}

fn check_originless_browser_request(headers: &HeaderMap) -> Result<(), &'static str> {
    match single_header(headers, HeaderName::from_static("sec-fetch-site"))? {
        None | Some("same-origin" | "none") => Ok(()),
        _ => Err("cross-origin browser request requires an approved origin"),
    }
}

fn local_aliases(port: u16) -> impl Iterator<Item = Authority> {
    ["localhost", "127.0.0.1", "[::1]"]
        .into_iter()
        .map(move |host| Authority {
            host: host.to_owned(),
            port,
        })
}

fn single_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, &'static str> {
    let mut values = headers.get_all(name).iter();
    let Some(value) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() {
        return Err("duplicate security header");
    }
    value
        .to_str()
        .map(Some)
        .map_err(|_| "invalid security header")
}

fn request_authority(
    headers: &HeaderMap,
    uri: &Uri,
    version: Version,
) -> Result<Authority, &'static str> {
    let host = single_header(headers, header::HOST)?
        .map(|raw| Authority::parse(raw, 80).ok_or("invalid host"))
        .transpose()?;
    if uri.scheme_str().is_some_and(|scheme| scheme != "http") {
        return Err("invalid request scheme");
    }
    let authority = uri
        .authority()
        .map(|raw| Authority::parse(raw.as_str(), 80).ok_or("invalid request authority"))
        .transpose()?;
    if let (Some(host), Some(authority)) = (&host, &authority)
        && host != authority
    {
        return Err("conflicting request authority");
    }
    if version != Version::HTTP_2 && version != Version::HTTP_3 && host.is_none() {
        return Err("host required");
    }
    host.or(authority).ok_or("host required")
}

#[cfg(test)]
#[path = "policy_tests.rs"]
mod tests;
