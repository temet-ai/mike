use super::{SettingApplies, SettingKind, SettingMeta, SettingScope};

pub const BACKEND_NAMES: &[&str] =
    &["herdr", "herdr-core", "tmux", "terminal", "ghostty", "none"];

const GROUP: &str = "Launcher";

fn entry(key: &'static str, kind: SettingKind, default: serde_json::Value) -> SettingMeta {
    SettingMeta {
        key,
        group: GROUP,
        kind,
        scope: SettingScope::Global,
        applies: SettingApplies::OnRestart,
        default,
        label: "",
        description: "",
        min_i64: None,
        max_i64: None,
        min_f64: None,
        max_f64: None,
        max_len: None,
        enum_variants: None,
    }
}

fn int(key: &'static str, default: u64, min: i64, max: i64) -> SettingMeta {
    let mut m = entry(key, SettingKind::Int, serde_json::Value::from(default));
    m.min_i64 = Some(min);
    m.max_i64 = Some(max);
    m
}

fn described(mut m: SettingMeta, label: &'static str, description: &'static str) -> SettingMeta {
    m.label = label;
    m.description = description;
    m
}

pub const MACHINE_KEYS: &[&str] = &[
    "launch_backend",
    "launch_backend_follow_caller",
    "launch_use_sidecar",
    "launch_proxy_forward_url",
    "launch_no_proxy",
    "launch_ca_cert",
    "launch_claude_args",
    "launch_worktree_config",
];

pub fn entries() -> Vec<SettingMeta> {
    let mut v = wiring_entries();
    v.extend(budget_entries());
    for m in &mut v {
        if MACHINE_KEYS.contains(&m.key) {
            m.scope = SettingScope::Machine;
        }
    }
    v
}

fn wiring_entries() -> Vec<SettingMeta> {
    let mut backend = entry(
        "launch_backend",
        SettingKind::Enum,
        serde_json::Value::String("none".to_owned()),
    );
    backend.enum_variants = Some(BACKEND_NAMES);
    vec![
        described(
            backend,
            "Terminal backend",
            "Where `mike spawn` opens a session's terminal surface when the caller's own terminal does not decide it (see the next setting). `herdr-core` is the Herdr desktop app. `none` means not chosen yet: every launch verb refuses until setup's terminal step (or MIKE_LAUNCH_BACKEND) picks one.",
        ),
        described(
            entry(
                "launch_backend_follow_caller",
                SettingKind::Bool,
                serde_json::Value::Bool(true),
            ),
            "Launch where the caller runs",
            "When on, a spawn opens in the terminal the caller runs in (a Herdr app terminal, an old herdr pane, tmux), else in whichever of those is running, else the terminal backend above. When off, the terminal backend above is always used.",
        ),
        described(
            entry(
                "launch_use_sidecar",
                SettingKind::Bool,
                serde_json::Value::Bool(false),
            ),
            "Route launched sessions through the sidecar",
            "When on, `mike claude` routes a launched session through the proxy-forward sidecar as HTTPS_PROXY; when off it points the session straight at this proxy.",
        ),
        described(
            entry(
                "launch_proxy_forward_url",
                SettingKind::String,
                serde_json::Value::String("http://127.0.0.1:9987".to_owned()),
            ),
            "Sidecar (HTTPS_PROXY) URL",
            "The proxy-forward sidecar a launched session routes through as HTTPS_PROXY; when it does not answer, `mike claude` falls back to ANTHROPIC_BASE_URL and the session has no Remote Control channel.",
        ),
        described(
            entry(
                "launch_no_proxy",
                SettingKind::String,
                serde_json::Value::String("localhost,127.0.0.1".to_owned()),
            ),
            "NO_PROXY for launched sessions",
            "Hosts a launched session reaches directly, bypassing the sidecar.",
        ),
        described(
            entry(
                "launch_ca_cert",
                SettingKind::String,
                serde_json::Value::String(String::new()),
            ),
            "Sidecar CA certificate",
            "Path handed to NODE_EXTRA_CA_CERTS. Blank resolves at use to proxy-forward's data dir (~/.local/share/proxy-forward/ca-cert.pem), then <cwd>/.deploy/proxy-forward-ca.pem.",
        ),
        described(
            entry(
                "launch_claude_args",
                SettingKind::StringList,
                serde_json::Value::Array(Vec::new()),
            ),
            "Operator claude flags",
            "Flags prepended to every launch's claude arguments, e.g. --dangerously-skip-permissions and --chrome. The caller's own arguments follow, in order.",
        ),
        described(
            entry(
                "launch_auto_trust",
                SettingKind::Bool,
                serde_json::Value::Bool(false),
            ),
            "Trust the folder before launch",
            "Record the folder as trusted in ~/.claude.json before the session starts, so Claude Code's trust dialog cannot swallow the delivered prompt. `--no-trust` on the verb refuses an untrusted folder instead.",
        ),
        described(
            entry(
                "launch_worktree_config",
                SettingKind::String,
                serde_json::Value::String(String::new()),
            ),
            "Worktree config carry-over files",
            "Colon-separated relative paths symlinked from the main checkout into a linked worktree when --worktree-ok is given. Blank uses the default three: .mcp.json, .env, .claude/settings.local.json.",
        ),
    ]
}

fn budget_entries() -> Vec<SettingMeta> {
    vec![
        described(
            int("launch_brief_inline_max", 800, 0, 1_000_000),
            "Brief inline limit (chars)",
            "Prompts longer than this are staged to a private brief file the session is told to read; the full text is stored on the launch row either way.",
        ),
        described(
            entry(
                "launch_channel_check_enabled",
                SettingKind::Bool,
                serde_json::Value::Bool(true),
            ),
            "Channel check after launch",
            "Wait for the launched session's Remote Control channel to link itself before reporting success; `timeout` is the one failing result.",
        ),
        described(
            int("launch_channel_check_timeout_secs", 90, 1, 3_600),
            "Channel check timeout (secs)",
            "How long a launch waits for the session's Remote Control channel before reporting `timeout` (the terminal is left open for inspection).",
        ),
        described(
            int("launch_channel_check_interval_secs", 3, 1, 60),
            "Channel check interval (secs)",
            "Seconds between channel-check polls of remote_control.channels.",
        ),
        described(
            int("launch_warn_surfaces", 8, 0, 1_000),
            "Surface warn threshold",
            "Warn on stderr when the backend already hosts this many session surfaces; 0 disables the warning.",
        ),
        described(
            int("launch_max_surfaces", 16, 0, 1_000),
            "Surface cap",
            "Refuse a launch when the backend already hosts this many session surfaces; 0 disables the cap.",
        ),
        described(
            int("launch_kill_wait_secs", 20, 0, 600),
            "Kill wait (secs)",
            "How long `mike spawn kill` waits for the claude process to exit after closing its surface before reporting process_exited: false.",
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::LaunchConfig;

    #[test]
    fn launch_entries_cover_every_launch_config_key_with_its_default() -> anyhow::Result<()> {
        let compiled = serde_json::to_value(LaunchConfig::default())?;
        let compiled = compiled
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("LaunchConfig must serialise to an object"))?;
        let entries = entries();
        for (key, value) in compiled {
            let meta = entries
                .iter()
                .find(|m| m.key == key)
                .ok_or_else(|| anyhow::anyhow!("{key} is not registered in SETTINGS_META"))?;
            let expected = if value.is_null() {
                serde_json::Value::String(String::new())
            } else {
                value.clone()
            };
            assert_eq!(meta.default, expected, "{key} default");
            assert!(!meta.label.is_empty(), "{key} label");
            assert!(!meta.description.is_empty(), "{key} description");
        }
        assert_eq!(entries.len(), compiled.len(), "no entry without a field");
        Ok(())
    }

    #[test]
    fn launch_machine_keys_are_machine_scoped_and_the_rest_shared() {
        for meta in entries() {
            let expected = if MACHINE_KEYS.contains(&meta.key) {
                SettingScope::Machine
            } else {
                SettingScope::Global
            };
            assert_eq!(meta.scope, expected, "{}", meta.key);
        }
        assert_eq!(
            entries()
                .iter()
                .filter(|m| m.scope == SettingScope::Machine)
                .count(),
            MACHINE_KEYS.len()
        );
    }

    #[test]
    fn launch_bool_entries_default_on_except_folder_trust() {
        for meta in entries() {
            if meta.kind == SettingKind::Bool {
                let expected = !matches!(meta.key, "launch_auto_trust" | "launch_use_sidecar");
                assert_eq!(
                    meta.default,
                    serde_json::Value::Bool(expected),
                    "{}",
                    meta.key
                );
            }
        }
    }
}
