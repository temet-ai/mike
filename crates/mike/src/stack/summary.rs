use crate::config::ProxyConfig;

use super::EnsureOutcome;

pub fn render_stack_summary(config: &ProxyConfig, outcome: &EnsureOutcome) -> String {
    let proxy = &config.proxy_listen_addr;
    let inspect = &config.inspect_listen_addr;

    let mut out = String::new();
    match outcome {
        EnsureOutcome::AlreadyRunning => {
            out.push_str("  mike stack: reusing the running server\n");
        }
        EnsureOutcome::Spawned { pid, log_path } => {
            out.push_str(&format!(
                "  mike stack: started server (pid {pid}, log {})\n",
                log_path.display()
            ));
        }
    }
    out.push_str(&format!("    Proxy:    http://{proxy}/v1\n"));
    out.push_str(&format!("    Inspect:  http://{inspect}/api\n"));
    if config.spa_enabled {
        out.push_str(&format!("    Web UI:   http://{}/\n", config.spa_bind));
    }
    out.push_str(&format!(
        "    Client:   ANTHROPIC_BASE_URL=http://{proxy}\n"
    ));
    out.push_str(
        "    Standalone: point any Anthropic-API client at that base URL — \
         `mike claude` is one option, not the only one.\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn summary_names_ports_and_client_hint() {
        let config = ProxyConfig::for_tests_in_memory();
        let summary = render_stack_summary(&config, &EnsureOutcome::AlreadyRunning);
        assert!(
            summary.contains(&config.proxy_listen_addr),
            "summary must name the proxy bind: {summary}"
        );
        assert!(
            summary.contains(&config.inspect_listen_addr),
            "summary must name the inspect bind: {summary}"
        );
        assert!(
            summary.contains("ANTHROPIC_BASE_URL=http://"),
            "summary must carry the client base-URL hint: {summary}"
        );
        assert!(
            summary.to_lowercase().contains("standalone"),
            "summary must present standalone usage alongside the wrapper: {summary}"
        );
    }

    #[test]
    fn summary_reports_spawned_pid_and_log() {
        let config = ProxyConfig::for_tests_in_memory();
        let outcome = EnsureOutcome::Spawned {
            pid: 4242,
            log_path: PathBuf::from("/tmp/mike-serve.log"),
        };
        let summary = render_stack_summary(&config, &outcome);
        assert!(summary.contains("4242"), "must report the pid: {summary}");
        assert!(
            summary.contains("/tmp/mike-serve.log"),
            "must report the log path: {summary}"
        );
    }
}
