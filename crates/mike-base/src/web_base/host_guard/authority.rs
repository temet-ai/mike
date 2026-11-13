use std::net::{Ipv4Addr, Ipv6Addr};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Authority {
    pub host: String,
    pub port: u16,
}

impl Authority {
    pub fn parse(raw: &str, default_port: u16) -> Option<Self> {
        let (host, port) = split_authority(raw)?;
        let host = canonical_host(host)?;
        let port = match port {
            Some(port) if !port.is_empty() && port.bytes().all(|byte| byte.is_ascii_digit()) => {
                port.parse().ok()?
            }
            None => default_port,
            _ => return None,
        };
        Some(Self { host, port })
    }
}

fn split_authority(raw: &str) -> Option<(&str, Option<&str>)> {
    if raw.starts_with('[') {
        let end = raw.find(']')?;
        let host = &raw[..=end];
        let tail = &raw[end + 1..];
        return Some((
            host,
            if tail.is_empty() {
                None
            } else {
                Some(tail.strip_prefix(':')?)
            },
        ));
    }
    match raw.split_once(':') {
        Some((host, port)) => Some((host, Some(port))),
        None => Some((raw, None)),
    }
}

fn canonical_host(raw: &str) -> Option<String> {
    if raw.starts_with('[') {
        let ip: Ipv6Addr = raw.strip_prefix('[')?.strip_suffix(']')?.parse().ok()?;
        return Some(format!("[{ip}]"));
    }
    if let Ok(ip) = raw.parse::<Ipv4Addr>() {
        return Some(ip.to_string());
    }
    if raw.is_empty() || raw.len() > 253 || raw.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
        return None;
    }
    raw.split('.')
        .all(valid_dns_label)
        .then(|| raw.to_ascii_lowercase())
}

fn valid_dns_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= 63
        && !label.starts_with('-')
        && !label.ends_with('-')
        && label
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Origin {
    pub scheme: String,
    pub authority: Authority,
}

impl Origin {
    pub fn parse(raw: &str) -> Option<Self> {
        let (scheme, authority) = raw.split_once("://")?;
        let scheme = scheme.to_ascii_lowercase();
        let default_port = match scheme.as_str() {
            "http" => 80,
            "https" => 443,
            _ => return None,
        };
        Some(Self {
            scheme,
            authority: Authority::parse(authority, default_port)?,
        })
    }

    pub fn serialize(&self) -> String {
        let default = if self.scheme == "https" { 443 } else { 80 };
        if self.authority.port == default {
            format!("{}://{}", self.scheme, self.authority.host)
        } else {
            format!(
                "{}://{}:{}",
                self.scheme, self.authority.host, self.authority.port
            )
        }
    }

    pub fn same_http_authority(&self, authority: &Authority) -> bool {
        self.scheme == "http" && &self.authority == authority
    }
}
