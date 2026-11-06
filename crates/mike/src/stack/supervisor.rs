use std::process::{Command, Stdio};

pub(super) const PROXY_UNIT: &str = "mike-stack-proxy.service";

pub(super) fn proxy_unit_installed() -> bool {
    systemctl_user(&["cat", PROXY_UNIT])
}

const SHARED_INSPECT_PORT: &str = "9989";

pub(super) fn is_shared_proxy_address(inspect_addr: &str) -> bool {
    inspect_addr
        .rsplit_once(':')
        .is_some_and(|(_, port)| port == SHARED_INSPECT_PORT)
}

pub(super) fn supervises(inspect_addr: &str) -> bool {
    is_shared_proxy_address(inspect_addr) && proxy_unit_installed()
}

pub(super) enum Supervised {
    DevUnit,
    Service(crate::service::InstalledProxy),
}

pub(super) fn supervised_by(inspect_addr: &str) -> Option<Supervised> {
    if supervises(inspect_addr) {
        return Some(Supervised::DevUnit);
    }
    crate::service::installed_proxy().map(Supervised::Service)
}

impl Supervised {
    pub(super) async fn restart(&self, config: &crate::config::ProxyConfig) -> anyhow::Result<()> {
        match self {
            Self::DevUnit => anyhow::ensure!(
                systemctl_user(&["restart", "--no-block", PROXY_UNIT]),
                "the development proxy service could not be restarted"
            ),
            Self::Service(_) => {
                let result = crate::service::restart_for_setup(config).await;
                if let Some(error) = result.error {
                    anyhow::bail!(error);
                }
            }
        }
        Ok(())
    }

    pub(super) async fn start(&self) {
        match self {
            Self::DevUnit => {
                start_proxy_unit();
            }
            Self::Service(_) => {
                crate::service::start_proxy().await;
            }
        }
    }

    pub(super) fn not_answering(&self, inspect_addr: &str, waited_secs: u64) -> String {
        match self {
            Self::DevUnit => not_answering_message(inspect_addr, waited_secs),
            Self::Service(service) => {
                service_not_answering_message(service, inspect_addr, waited_secs)
            }
        }
    }
}

pub(super) fn service_not_answering_message(
    service: &crate::service::InstalledProxy,
    inspect_addr: &str,
    waited_secs: u64,
) -> String {
    let log = service.log.as_ref().map_or_else(String::new, |path| {
        format!(" and the log at {}", path.display())
    });
    format!(
        "the proxy runs as the background service {} under {} and has not answered on {inspect_addr} for {waited_secs}s; mike does not start a second proxy beside it, because a second instance on the shared database marks every other session's in-flight requests as crashed. Check `mike service status`{log}",
        service.identifier, service.manager_name
    )
}

pub(super) fn start_proxy_unit() -> bool {
    systemctl_user(&["start", "--no-block", PROXY_UNIT])
}

fn systemctl_user(args: &[&str]) -> bool {
    Command::new("systemctl")
        .arg("--user")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

pub(super) fn not_answering_message(inspect_addr: &str, waited_secs: u64) -> String {
    format!(
        "the proxy runs under the systemd user unit {PROXY_UNIT} and has not answered on {inspect_addr} for {waited_secs}s; mike does not start a second proxy beside it, because a second instance on the shared database marks every other session's in-flight requests as crashed. Check `systemctl --user status {PROXY_UNIT}` and .deploy/proxy.log"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shared_inspect_address_is_guarded() {
        assert!(is_shared_proxy_address("127.0.0.1:9989"));
        assert!(is_shared_proxy_address("localhost:9989"));
    }

    #[test]
    fn a_test_port_proxy_is_not_guarded() {
        assert!(!is_shared_proxy_address("127.0.0.1:19989"));
        assert!(!is_shared_proxy_address("127.0.0.1:9990"));
        assert!(!is_shared_proxy_address("no-port"));
    }

    #[test]
    fn service_not_answering_message_names_the_service_the_address_and_the_log() {
        let service = crate::service::InstalledProxy {
            manager_name: "launchd",
            identifier: "ai.temet.mike.proxy",
            log: Some(std::path::PathBuf::from("/state/logs/proxy.log")),
        };
        let text = service_not_answering_message(&service, "127.0.0.1:9989", 45);
        assert!(text.contains("ai.temet.mike.proxy under launchd"));
        assert!(text.contains("127.0.0.1:9989"));
        assert!(text.contains("45s"));
        assert!(text.contains("does not start a second proxy"));
        assert!(text.contains("mike service status"));
        assert!(text.contains("/state/logs/proxy.log"));
    }

    #[test]
    fn not_answering_message_names_the_unit_the_address_and_the_wait() {
        let text = not_answering_message("127.0.0.1:9989", 45);
        assert!(text.contains(PROXY_UNIT));
        assert!(text.contains("127.0.0.1:9989"));
        assert!(text.contains("45s"));
        assert!(text.contains("does not start a second proxy"));
    }
}
