use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::Rect;

use super::SetupState;
use super::step::{Cx, Env, WizardStep};
use crate::claude_code::Locations;
use crate::config::ProxyConfig;
use crate::tui::client::ApiClient;
use crate::tui::test_support::render_to_string;

pub(super) struct Harness {
    pub(super) dir: tempfile::TempDir,
    pub(super) config: ProxyConfig,
    pub(super) client: ApiClient,
    pub(super) env: Env,
}

impl Harness {
    pub(super) fn new() -> anyhow::Result<Self> {
        let dir = tempfile::tempdir()?;
        let root = std::fs::canonicalize(dir.path())?;
        std::fs::create_dir_all(root.join("home"))?;
        std::fs::create_dir_all(root.join("bin"))?;
        let path_var: OsString = root.join("bin").into_os_string();
        let locations = Locations::for_home(&root.join("home"), Some(path_var));
        let mut config = ProxyConfig::for_tests_in_memory();
        config.config_path = locations.config_file.clone();
        config.inspect_listen_addr = "127.0.0.1:1".to_string();
        config.proxy_listen_addr = "127.0.0.1:2".to_string();
        config.launch.proxy_forward_url = "http://127.0.0.1:3".to_string();
        let client = ApiClient::new(&config);
        Ok(Self {
            dir,
            config,
            client,
            env: Env::with_locations(locations),
        })
    }

    pub(super) fn home(&self) -> PathBuf {
        self.env.locations.home.clone()
    }

    pub(super) fn bin(&self) -> PathBuf {
        self.home().with_file_name("bin")
    }

    pub(super) fn add_executable(&self, name: &str, body: &str) -> anyhow::Result<PathBuf> {
        let path = self.bin().join(name);
        std::fs::write(&path, body)?;
        make_executable(&path)?;
        Ok(path)
    }

    pub(super) fn cx(&mut self) -> Cx<'_> {
        Cx {
            env: &mut self.env,
            client: &self.client,
            config: &mut self.config,
        }
    }

    pub(super) fn config_text(&self) -> String {
        std::fs::read_to_string(self.env.config_file()).unwrap_or_default()
    }

    pub(super) fn into_state(
        self,
        steps: Vec<Box<dyn WizardStep>>,
        current: usize,
    ) -> (SetupState, ApiClient, ProxyConfig, tempfile::TempDir) {
        (
            SetupState::with_parts(self.env, steps, current),
            self.client,
            self.config,
            self.dir,
        )
    }
}

#[cfg(unix)]
fn make_executable(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
    Ok(())
}

#[cfg(not(unix))]
fn make_executable(_: &Path) -> anyhow::Result<()> {
    Ok(())
}

pub(super) fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

pub(super) fn typed(text: &str) -> Vec<KeyEvent> {
    text.chars().map(|c| key(KeyCode::Char(c))).collect()
}

pub(super) fn render_step(step: &dyn WizardStep) -> String {
    render_to_string(100, 40, |frame: &mut Frame, area: Rect| {
        step.draw(frame, area)
    })
}

pub(super) fn words(text: &str) -> std::collections::HashSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}
