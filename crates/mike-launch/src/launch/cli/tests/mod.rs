#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]

pub mod doctor_tests;
pub mod fleet_settings_tests;
pub mod kill_tests;
pub mod list_tests;
pub mod open_prompt_tests;
pub mod open_tests;
pub mod prompt_turn_tests;
pub mod proxy_ledger_tests;
pub mod queue_brief_tests;
pub mod queue_tests;

use std::path::{Path, PathBuf};

use super::ledger::Ledger;
use super::runner::Runner;
use crate::config::LaunchConfig;
use crate::launch::backend::fake::FakeBackend;
use crate::launch::{TerminalBackend, TrustGate};

pub const MIKE_USER_SERVER: &str = r#"{"mcpServers":{"mike":{"command":"mike-mcp"}}}"#;

pub fn toks(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

pub fn runner<'a>(
    backend: &'a dyn TerminalBackend,
    config: &'a LaunchConfig,
    home: &Path,
) -> Runner<'a> {
    let claude_json = home.join(".claude.json");
    if !claude_json.exists() {
        std::fs::write(&claude_json, MIKE_USER_SERVER).unwrap();
    }
    Runner {
        backend,
        ledger: Ledger::none(),
        config,
        proxy: None,
        trust: TrustGate::new(claude_json),
        briefs_dir: home.join("briefs"),
        inspect_addr: None,
        mcp_server: None,
        backend_reason: None,
    }
}

pub fn project_dir(home: &Path) -> PathBuf {
    let dir = home.join("proj");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::canonicalize(dir).unwrap()
}

pub fn fake() -> FakeBackend {
    FakeBackend::available()
}

pub fn json(line: &str) -> serde_json::Value {
    serde_json::from_str(line).unwrap()
}

pub fn assert_key_order(line: &str, expected: &[&str]) {
    let positions: Vec<usize> = expected
        .iter()
        .map(|k| {
            line.find(&format!("\"{k}\":"))
                .unwrap_or_else(|| panic!("key {k} missing from {line}"))
        })
        .collect();
    assert!(
        positions.windows(2).all(|w| w[0] < w[1]),
        "key order in {line}"
    );
    let value = json(line);
    assert_eq!(value.as_object().unwrap().len(), expected.len(), "{line}");
}
