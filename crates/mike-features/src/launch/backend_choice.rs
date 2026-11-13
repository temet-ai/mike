use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::backend::{BackendId, TerminalBackend, backend_for};
use super::error::LaunchError;
use super::herdr_core::HerdrCoreBackend;

pub const CALLER_ENV_KEYS: [&str; 4] = ["HC_TERMINAL_ID", "HERDR_PANE_ID", "TMUX_PANE", "TMUX"];

const SOCKET_TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallerTerminal {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub herdr_core: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub herdr_pane: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tmux_pane: Option<String>,
}

impl CallerTerminal {
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Self {
        let get = |key: &str| lookup(key).filter(|v| !v.trim().is_empty());
        Self {
            herdr_core: get("HC_TERMINAL_ID"),
            herdr_pane: get("HERDR_PANE_ID"),
            tmux_pane: get("TMUX_PANE").or_else(|| get("TMUX").map(|_| "tmux".to_string())),
        }
    }

    pub fn from_env() -> Self {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    pub fn from_env_map(env: &BTreeMap<String, String>) -> Self {
        Self::from_lookup(|key| env.get(key).cloned())
    }

    pub fn is_empty(&self) -> bool {
        self.herdr_core.is_none() && self.herdr_pane.is_none() && self.tmux_pane.is_none()
    }

    fn backend(&self) -> Option<(BackendId, String)> {
        if let Some(t) = &self.herdr_core {
            return Some((
                BackendId::HerdrCore,
                format!("the caller runs in Herdr app terminal {t}"),
            ));
        }
        if let Some(p) = &self.herdr_pane {
            return Some((
                BackendId::Herdr,
                format!("the caller runs in herdr pane {p}"),
            ));
        }
        self.tmux_pane
            .as_ref()
            .map(|p| (BackendId::Tmux, format!("the caller runs in tmux pane {p}")))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BackendChoice {
    pub id: BackendId,
    pub reason: String,
    #[serde(skip)]
    pub caller_terminal: Option<String>,
}

impl BackendChoice {
    pub fn configured(id: BackendId) -> Self {
        Self {
            id,
            reason: format!("launch_backend is {id}"),
            caller_terminal: None,
        }
    }

    pub fn backend(&self) -> Option<Box<dyn TerminalBackend>> {
        match self.id {
            BackendId::HerdrCore => Some(Box::new(
                HerdrCoreBackend::new().with_caller(self.caller_terminal.clone()),
            )),
            other => backend_for(other),
        }
    }
}

pub struct ChoiceInputs<'a> {
    pub explicit: Option<BackendId>,
    pub follow_caller: bool,
    pub configured: BackendId,
    pub caller: &'a CallerTerminal,
    pub running: &'a [BackendId],
}

pub fn choose(inputs: &ChoiceInputs<'_>) -> BackendChoice {
    let configured = inputs.configured;
    let pick = |id: BackendId, reason: String| BackendChoice {
        id,
        reason,
        caller_terminal: inputs
            .caller
            .herdr_core
            .clone()
            .filter(|_| id == BackendId::HerdrCore),
    };
    if let Some(id) = inputs.explicit {
        return pick(id, format!("the request asked for {id}"));
    }
    if !inputs.follow_caller {
        return pick(
            configured,
            format!("launch_backend_follow_caller is off; launch_backend is {configured}"),
        );
    }
    if let Some((id, reason)) = inputs.caller.backend() {
        return pick(id, reason);
    }
    if let Some(id) = inputs.running.first() {
        return pick(*id, format!("no caller terminal; {id} is running"));
    }
    pick(
        configured,
        format!(
            "no caller terminal and no terminal server answers; launch_backend is {configured}"
        ),
    )
}

pub async fn resolve(
    config: &crate::config::LaunchConfig,
    explicit: Option<BackendId>,
    caller: &CallerTerminal,
) -> Result<(BackendChoice, Box<dyn TerminalBackend>), LaunchError> {
    let configured: BackendId = config.backend.parse()?;
    let follow = config.follow_caller && configured != BackendId::None;
    let running = if follow && explicit.is_none() && caller.is_empty() {
        running_backends().await
    } else {
        Vec::new()
    };
    let choice = choose(&ChoiceInputs {
        explicit,
        follow_caller: follow,
        configured,
        caller,
        running: &running,
    });
    let backend = choice
        .backend()
        .ok_or_else(|| LaunchError::BackendUnavailable {
            backend: choice.id,
            detail: "this build of mike has no adapter for it yet".into(),
            remedy: Some(
                "choose `herdr-core`, `herdr` (or `none`) in setup's terminal step or MIKE_LAUNCH_BACKEND"
                    .into(),
            ),
        })?;
    Ok((choice, backend))
}

pub async fn running_backends() -> Vec<BackendId> {
    let mut running = Vec::new();
    for (id, socket) in candidate_sockets() {
        if let Some(socket) = socket
            && answers(&socket).await
        {
            running.push(id);
        }
    }
    running
}

fn candidate_sockets() -> [(BackendId, Option<PathBuf>); 3] {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .or_else(|| uid().map(|u| PathBuf::from(format!("/run/user/{u}"))));
    let tmux_dir = std::env::var_os("TMUX_TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    [
        (
            BackendId::HerdrCore,
            runtime.map(|r| r.join("herdr-core/default.sock")),
        ),
        (
            BackendId::Herdr,
            std::env::var_os("HERDR_SOCKET_PATH")
                .map(PathBuf::from)
                .or_else(|| home.map(|h| h.join(".config/herdr/herdr.sock"))),
        ),
        (
            BackendId::Tmux,
            uid().map(|u| tmux_dir.join(format!("tmux-{u}/default"))),
        ),
    ]
}

fn uid() -> Option<u32> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(std::env::var_os("HOME")?)
        .ok()
        .map(|m| m.uid())
}

async fn answers(socket: &std::path::Path) -> bool {
    matches!(
        tokio::time::timeout(SOCKET_TIMEOUT, tokio::net::UnixStream::connect(socket)).await,
        Ok(Ok(_))
    )
}

#[cfg(test)]
pub mod tests {
    use super::*;

    fn caller(pairs: &[(&str, &str)]) -> CallerTerminal {
        let env: BTreeMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        CallerTerminal::from_env_map(&env)
    }

    fn inputs<'a>(caller: &'a CallerTerminal, running: &'a [BackendId]) -> ChoiceInputs<'a> {
        ChoiceInputs {
            explicit: None,
            follow_caller: true,
            configured: BackendId::Herdr,
            caller,
            running,
        }
    }

    #[test]
    fn choose_caller_in_a_herdr_app_terminal_opens_in_the_app_with_that_terminal() {
        let c = caller(&[("HC_TERMINAL_ID", "t13"), ("HERDR_PANE_ID", "w:p")]);
        let choice = choose(&inputs(&c, &[]));
        assert_eq!(choice.id, BackendId::HerdrCore);
        assert_eq!(choice.caller_terminal.as_deref(), Some("t13"));
        assert!(choice.reason.contains("t13"), "{}", choice.reason);
    }

    #[test]
    fn choose_caller_in_old_herdr_or_tmux_keeps_that_backend() {
        let herdr = caller(&[("HERDR_PANE_ID", "wA6:p2"), ("TMUX", "/tmp/x")]);
        assert_eq!(choose(&inputs(&herdr, &[])).id, BackendId::Herdr);
        let tmux = caller(&[("TMUX_PANE", "%3")]);
        let choice = choose(&inputs(&tmux, &[BackendId::HerdrCore]));
        assert_eq!(choice.id, BackendId::Tmux);
        assert_eq!(choice.caller_terminal, None);
    }

    #[test]
    fn choose_no_caller_prefers_the_herdr_app_over_a_running_configured_backend() {
        let c = CallerTerminal::default();
        let running = [BackendId::HerdrCore, BackendId::Herdr];
        assert_eq!(choose(&inputs(&c, &running)).id, BackendId::HerdrCore);
    }

    #[test]
    fn choose_no_caller_takes_the_first_running_backend_else_the_configured() {
        let c = CallerTerminal::default();
        let choice = choose(&inputs(&c, &[BackendId::HerdrCore, BackendId::Tmux]));
        assert_eq!(choice.id, BackendId::HerdrCore);
        assert!(choice.reason.contains("running"), "{}", choice.reason);
        let fallback = choose(&inputs(&c, &[]));
        assert_eq!(fallback.id, BackendId::Herdr);
        assert!(
            fallback.reason.contains("launch_backend"),
            "{}",
            fallback.reason
        );
    }

    #[test]
    fn choose_explicit_request_or_follow_off_overrides_the_caller() {
        let c = caller(&[("HC_TERMINAL_ID", "t13")]);
        let mut explicit = inputs(&c, &[]);
        explicit.explicit = Some(BackendId::Tmux);
        assert_eq!(choose(&explicit).id, BackendId::Tmux);
        let mut pinned = inputs(&c, &[]);
        pinned.follow_caller = false;
        let choice = choose(&pinned);
        assert_eq!(choice.id, BackendId::Herdr);
        assert!(
            choice.reason.contains("follow_caller is off"),
            "{}",
            choice.reason
        );
    }

    #[test]
    fn caller_blank_values_count_as_absent() {
        let c = caller(&[("HC_TERMINAL_ID", " "), ("TMUX_PANE", "")]);
        assert!(c.is_empty());
    }

    #[test]
    fn backend_choice_builds_a_backend_for_every_adapter() {
        for id in [BackendId::HerdrCore, BackendId::Herdr, BackendId::Tmux] {
            let backend = BackendChoice::configured(id).backend();
            assert_eq!(backend.map(|b| b.id()), Some(id));
        }
    }
}
