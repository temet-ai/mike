use std::time::Duration;

use crate::config::ProxyConfig;

use super::protocol::RuntimeStatus;

const HEALTH_TIMEOUT: Duration = Duration::from_secs(2);

async fn fetch(address: &str, allow_remote: bool) -> Option<serde_json::Value> {
    let endpoint = crate::config::endpoint::http_url(address, allow_remote).ok()?;
    let client = reqwest::Client::builder()
        .timeout(HEALTH_TIMEOUT)
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .ok()?;
    let response = client
        .get(format!("{endpoint}/api/health"))
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.json().await.ok()
}

pub(crate) async fn probe(address: &str) -> bool {
    fetch(address, false).await.is_some_and(|body| {
        body["build"]["sha"].is_string() && body["process"]["uptime_seconds"].is_u64()
    })
}

pub(crate) async fn verified(config: &ProxyConfig, status: &RuntimeStatus) -> bool {
    fetch(
        &config.inspect_listen_addr,
        config.unsupported_allow_remote_listeners,
    )
    .await
    .is_some_and(|body| matches(&body, status))
}

fn matches(body: &serde_json::Value, status: &RuntimeStatus) -> bool {
    status.ready
        && status.matches_requested
        && !status.stopping
        && body["instance"]["instance_id"].as_str() == Some(status.identity.instance_id.as_str())
        && body["instance"]["boot_generation"].as_str()
            == Some(status.identity.generation.to_string().as_str())
        && body["persistence"]["enabled"].as_bool() == Some(status.persist_enabled)
        && (!status.persist_enabled || body["persistence"]["reachable"].as_bool() == Some(true))
}

#[cfg(test)]
mod tests {
    use super::super::protocol::InstanceIdentity;
    use super::*;

    fn status() -> RuntimeStatus {
        RuntimeStatus {
            identity: InstanceIdentity {
                instance_id: "a".into(),
                generation: uuid::Uuid::nil(),
                pid: 42,
                started_unix_secs: 123,
            },
            ready: true,
            stopping: false,
            matches_requested: true,
            bound_listeners: None,
            persist_enabled: true,
        }
    }

    #[test]
    fn foreign_http_200_and_wrong_mode_or_generation_are_not_readiness() {
        let mut state = status();
        assert!(!matches(
            &serde_json::json!({"status":"ok", "build":{"sha":"x"}, "process":{"uptime_seconds":1}}),
            &state
        ));
        let mut body = serde_json::json!({"instance":{"instance_id":"a","boot_generation":uuid::Uuid::nil()},"persistence":{"enabled":true,"reachable":true}});
        assert!(matches(&body, &state));
        state.matches_requested = false;
        assert!(!matches(&body, &state));
        state.matches_requested = true;
        body["persistence"]["enabled"] = false.into();
        assert!(!matches(&body, &state));
        body["persistence"]["enabled"] = true.into();
        body["instance"]["boot_generation"] = uuid::Uuid::new_v4().to_string().into();
        assert!(!matches(&body, &state));
    }
}
