use std::path::PathBuf;

use crate::service::spec::{ServiceKind, Spec};
use crate::service::{launchd, systemd};

fn spec(kind: ServiceKind, exe: &str, state: &str, path: &str) -> Spec {
    Spec {
        kind,
        exe: PathBuf::from(exe),
        state_dir: PathBuf::from(state),
        environment: vec![("PATH".to_string(), path.to_string())],
    }
}

#[test]
fn launchd_proxy_agent_matches_the_expected_plist() {
    let spec = spec(
        ServiceKind::Proxy,
        "/opt/homebrew/bin/mike",
        "/Users/ada/.local/state/mike",
        "/opt/homebrew/bin:/usr/bin",
    );
    let declaration = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>";
    let doctype = "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">";
    let body = "<!-- Managed by mike service install. Run `mike service install` to regenerate this file and `mike service uninstall` to remove it. -->
<plist version=\"1.0\">
<dict>
  <key>Label</key>
  <string>ai.temet.mike.proxy</string>
  <key>ProgramArguments</key>
  <array>
    <string>/opt/homebrew/bin/mike</string>
    <string>serve</string>
  </array>
  <key>WorkingDirectory</key>
  <string>/Users/ada/.local/state/mike</string>
  <key>EnvironmentVariables</key>
  <dict>
    <key>PATH</key>
    <string>/opt/homebrew/bin:/usr/bin</string>
  </dict>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
  <key>StandardOutPath</key>
  <string>/Users/ada/.local/state/mike/logs/proxy.log</string>
  <key>StandardErrorPath</key>
  <string>/Users/ada/.local/state/mike/logs/proxy.log</string>
</dict>
</plist>
";
    let expected = format!("{declaration}\n{doctype}\n{body}");
    assert_eq!(launchd::render(&spec), expected);
}

#[test]
fn launchd_sidecar_agent_runs_mike_forward_serve_under_its_own_label() {
    let spec = spec(
        ServiceKind::Sidecar,
        "/opt/homebrew/bin/mike",
        "/Users/ada/.local/state/mike",
        "/usr/bin",
    );
    let plist = launchd::render(&spec);
    assert!(
        plist.contains("<string>ai.temet.mike.forward</string>"),
        "{plist}"
    );
    assert!(
        plist.contains("<string>forward</string>\n    <string>serve</string>"),
        "{plist}"
    );
    assert!(plist.contains("logs/sidecar.log"), "{plist}");
}

#[test]
fn launchd_values_are_xml_escaped() {
    let spec = spec(
        ServiceKind::Proxy,
        "/opt/a&b/mike",
        "/Users/a<b>/state",
        "/usr/bin",
    );
    let plist = launchd::render(&spec);
    assert!(
        plist.contains("<string>/opt/a&amp;b/mike</string>"),
        "{plist}"
    );
    assert!(
        plist.contains("<string>/Users/a&lt;b&gt;/state</string>"),
        "{plist}"
    );
}

#[test]
fn systemd_proxy_unit_matches_the_expected_text() {
    let spec = spec(
        ServiceKind::Proxy,
        "/usr/bin/mike",
        "/home/ada/.local/state/mike",
        "/usr/local/bin:/usr/bin",
    );
    let expected = "# Managed by mike service install. Run `mike service install` to regenerate this file and `mike service uninstall` to remove it.
[Unit]
Description=mike proxy, inspect API and web UI

[Service]
Type=simple
ExecStart=/usr/bin/mike serve
WorkingDirectory=/home/ada/.local/state/mike
Environment=\"PATH=/usr/local/bin:/usr/bin\"
Restart=on-failure
RestartSec=3
StandardOutput=append:/home/ada/.local/state/mike/logs/proxy.log
StandardError=append:/home/ada/.local/state/mike/logs/proxy.log

[Install]
WantedBy=default.target
";
    assert_eq!(systemd::render(&spec), expected);
}

#[test]
fn systemd_sidecar_unit_starts_after_the_proxy_unit() {
    let spec = spec(
        ServiceKind::Sidecar,
        "/usr/bin/mike",
        "/home/ada/.local/state/mike",
        "/usr/bin",
    );
    let unit = systemd::render(&spec);
    assert!(unit.contains("After=mike.service\n"), "{unit}");
    assert!(
        unit.contains("ExecStart=/usr/bin/mike forward serve\n"),
        "{unit}"
    );
    assert!(
        unit.contains("Description=mike forwarding sidecar\n"),
        "{unit}"
    );
}

#[test]
fn systemd_quotes_paths_with_spaces_and_escapes_specifiers() {
    let spec = spec(
        ServiceKind::Proxy,
        "/opt/my tools/mike",
        "/home/a%b/state",
        "/usr/bin",
    );
    let unit = systemd::render(&spec);
    assert!(
        unit.contains("ExecStart=\"/opt/my tools/mike\" serve\n"),
        "{unit}"
    );
    assert!(
        unit.contains("WorkingDirectory=/home/a%%b/state\n"),
        "{unit}"
    );
}

#[test]
fn both_renderings_carry_the_marker_that_uninstall_looks_for() {
    let spec = spec(ServiceKind::Proxy, "/usr/bin/mike", "/state", "/usr/bin");
    assert!(crate::service::files::is_ours(&launchd::render(&spec)));
    assert!(crate::service::files::is_ours(&systemd::render(&spec)));
}
