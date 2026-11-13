use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use axum::Json;
use axum::extract::State;
use serde::Serialize;

use crate::state::AppState;

#[derive(Serialize)]
pub(crate) struct ConnectionInfo {
    proxy_url: Option<String>,
    inspect_url: Option<String>,
    browser_url: Option<String>,
}

pub(crate) async fn connection_info(State(state): State<AppState>) -> Json<ConnectionInfo> {
    let config = state.config();
    Json(ConnectionInfo {
        proxy_url: reachable_url(&config.proxy_listen_addr),
        inspect_url: reachable_url(&config.inspect_listen_addr),
        browser_url: config
            .spa_enabled
            .then(|| reachable_url(&config.spa_bind))
            .flatten(),
    })
}

fn reachable_url(address: &str) -> Option<String> {
    if let Ok(mut socket) = address.parse::<SocketAddr>() {
        if socket.port() == 0 {
            return None;
        }
        if socket.ip().is_unspecified() {
            socket.set_ip(match socket.ip() {
                IpAddr::V4(_) => IpAddr::V4(Ipv4Addr::LOCALHOST),
                IpAddr::V6(_) => IpAddr::V6(Ipv6Addr::LOCALHOST),
            });
        }
        return Some(format!("http://{socket}"));
    }
    let port = address.strip_prefix("localhost:")?.parse::<u16>().ok()?;
    (port != 0).then(|| format!("http://localhost:{port}"))
}

#[cfg(test)]
mod tests {
    use super::reachable_url;

    #[test]
    fn endpoint_urls_are_client_reachable_without_reflected_input() {
        for (input, expected) in [
            ("0.0.0.0:1234", Some("http://127.0.0.1:1234")),
            ("[::]:1234", Some("http://[::1]:1234")),
            ("127.0.0.1:4567", Some("http://127.0.0.1:4567")),
            ("localhost:9990", Some("http://localhost:9990")),
            ("127.0.0.1:0", None),
            ("user:password@localhost:80", None),
            ("example.com:80/path", None),
            ("bad-value", None),
        ] {
            assert_eq!(reachable_url(input).as_deref(), expected);
        }
    }
}
