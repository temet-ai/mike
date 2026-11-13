use super::schema::ProxyConfig;

pub fn apply_launch_override(cfg: &mut ProxyConfig, key: &str, value: &serde_json::Value) -> bool {
    let launch = &mut cfg.launch;
    match key {
        "launch_backend" => set_string(&mut launch.backend, value),
        "launch_backend_follow_caller" => set_bool(&mut launch.follow_caller, value),
        "launch_proxy_forward_url" => set_string(&mut launch.proxy_forward_url, value),
        "launch_no_proxy" => set_string(&mut launch.no_proxy, value),
        "launch_ca_cert" => {
            if let Some(v) = value.as_str() {
                launch.ca_cert = (!v.trim().is_empty()).then(|| v.to_owned());
            }
        }
        "launch_claude_args" => {
            if let Some(items) = value.as_array() {
                launch.claude_args = items
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect();
            }
        }
        "launch_brief_inline_max" => {
            if let Some(v) = value.as_u64().and_then(|v| usize::try_from(v).ok()) {
                launch.brief_inline_max = v;
            }
        }
        "launch_channel_check_enabled" => set_bool(&mut launch.channel_check_enabled, value),
        "launch_channel_check_timeout_secs" => {
            set_u64(&mut launch.channel_check_timeout_secs, value)
        }
        "launch_channel_check_interval_secs" => {
            set_u64(&mut launch.channel_check_interval_secs, value)
        }
        "launch_warn_surfaces" => set_u32(&mut launch.warn_surfaces, value),
        "launch_max_surfaces" => set_u32(&mut launch.max_surfaces, value),
        "launch_auto_trust" => set_bool(&mut launch.auto_trust, value),
        "launch_kill_wait_secs" => set_u64(&mut launch.kill_wait_secs, value),
        "launch_worktree_config" => {
            if let Some(v) = value.as_str() {
                launch.worktree_config = (!v.trim().is_empty()).then(|| v.to_owned());
            }
        }
        _ => return false,
    }
    true
}

fn set_string(field: &mut String, value: &serde_json::Value) {
    if let Some(v) = value.as_str() {
        *field = v.to_owned();
    }
}

fn set_bool(field: &mut bool, value: &serde_json::Value) {
    if let Some(v) = value.as_bool() {
        *field = v;
    }
}

fn set_u64(field: &mut u64, value: &serde_json::Value) {
    if let Some(v) = value.as_u64() {
        *field = v;
    }
}

fn set_u32(field: &mut u32, value: &serde_json::Value) {
    if let Some(v) = value.as_u64().and_then(|v| u32::try_from(v).ok()) {
        *field = v;
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn folded(rows: &[(&str, serde_json::Value)]) -> ProxyConfig {
        let mut cfg = ProxyConfig::for_tests_in_memory();
        for (key, value) in rows {
            assert!(apply_launch_override(&mut cfg, key, value), "{key}");
        }
        cfg
    }

    #[test]
    fn apply_launch_override_folds_the_wiring_keys() {
        let cfg = folded(&[
            ("launch_backend", json!("herdr")),
            ("launch_proxy_forward_url", json!("http://127.0.0.1:1")),
            ("launch_no_proxy", json!("localhost")),
            ("launch_ca_cert", json!("/ca.pem")),
            ("launch_claude_args", json!(["--chrome", 7, "--verbose"])),
            ("launch_auto_trust", json!(false)),
        ]);
        let l = &cfg.launch;
        assert_eq!(l.backend, "herdr");
        assert_eq!(l.proxy_forward_url, "http://127.0.0.1:1");
        assert_eq!(l.no_proxy, "localhost");
        assert_eq!(l.ca_cert.as_deref(), Some("/ca.pem"));
        assert_eq!(l.claude_args, vec!["--chrome", "--verbose"]);
        assert!(!l.auto_trust);
    }

    #[test]
    fn apply_launch_override_folds_the_budget_keys() {
        let cfg = folded(&[
            ("launch_brief_inline_max", json!(100)),
            ("launch_channel_check_enabled", json!(false)),
            ("launch_channel_check_timeout_secs", json!(10)),
            ("launch_channel_check_interval_secs", json!(1)),
            ("launch_warn_surfaces", json!(2)),
            ("launch_max_surfaces", json!(3)),
            ("launch_kill_wait_secs", json!(4)),
        ]);
        let l = &cfg.launch;
        assert_eq!(l.brief_inline_max, 100);
        assert!(!l.channel_check_enabled);
        assert_eq!(l.channel_check_timeout_secs, 10);
        assert_eq!(l.channel_check_interval_secs, 1);
        assert_eq!(l.warn_surfaces, 2);
        assert_eq!(l.max_surfaces, 3);
        assert_eq!(l.kill_wait_secs, 4);
    }

    #[test]
    fn apply_launch_override_blank_ca_cert_unsets_and_unknown_key_is_false() {
        let mut cfg = ProxyConfig::for_tests_in_memory();
        cfg.launch.ca_cert = Some("/old.pem".into());
        assert!(apply_launch_override(
            &mut cfg,
            "launch_ca_cert",
            &json!("  ")
        ));
        assert_eq!(cfg.launch.ca_cert, None);
        assert!(!apply_launch_override(
            &mut cfg,
            "log_level",
            &json!("debug")
        ));
    }

    #[test]
    fn apply_launch_override_worktree_config_sets_and_blank_unsets() {
        let mut cfg = ProxyConfig::for_tests_in_memory();
        assert!(apply_launch_override(
            &mut cfg,
            "launch_worktree_config",
            &json!("a.toml:b.json")
        ));
        assert_eq!(cfg.launch.worktree_config.as_deref(), Some("a.toml:b.json"));
        assert!(apply_launch_override(
            &mut cfg,
            "launch_worktree_config",
            &json!("  ")
        ));
        assert_eq!(cfg.launch.worktree_config, None);
    }

    #[test]
    fn apply_launch_override_wrong_type_leaves_field_alone() {
        let mut cfg = ProxyConfig::for_tests_in_memory();
        assert!(apply_launch_override(
            &mut cfg,
            "launch_max_surfaces",
            &json!("many")
        ));
        assert_eq!(cfg.launch.max_surfaces, 16);
        assert!(apply_launch_override(
            &mut cfg,
            "launch_auto_trust",
            &json!(1)
        ));
        assert!(!cfg.launch.auto_trust);
    }
}
