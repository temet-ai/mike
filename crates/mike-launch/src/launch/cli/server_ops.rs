use super::doctor::{self, DoctorReport};
use super::kill::{self, KillReport};
use super::ledger::Ledger;
use super::proxy_env::resolve_proxy_env;
use super::runner::Runner;
use crate::base_state::BaseState;
use crate::launch::backend_choice::{self, BackendChoice, CallerTerminal};
use crate::launch::{BackendId, LaunchError, TerminalBackend, TrustGate, brief};
use uuid::Uuid;

pub async fn resolve_backend(
    state: &BaseState,
    explicit: Option<BackendId>,
    caller: &CallerTerminal,
) -> Result<(BackendChoice, Box<dyn TerminalBackend>), LaunchError> {
    backend_choice::resolve(&state.config.launch, explicit, caller).await
}

pub async fn configured_backend(
    state: &BaseState,
) -> Result<Box<dyn TerminalBackend>, LaunchError> {
    let (_, backend) = resolve_backend(state, None, &CallerTerminal::default()).await?;
    Ok(backend)
}

pub async fn build_runner<'a>(
    state: &'a BaseState,
    backend: &'a dyn TerminalBackend,
) -> Result<Runner<'a>, LaunchError> {
    let cwd = std::env::current_dir().ok();
    Ok(Runner {
        backend,
        ledger: Ledger::new(state.persist().cloned())
            .with_mail_briefs(state.feature_toggle_overlay.load().launch_brief)
            .with_queue(super::queue::LocalQueue::default_dir()),
        config: &state.config.launch,
        proxy: resolve_proxy_env(&state.config.launch, cwd.as_deref()),
        trust: TrustGate::from_home()?,
        briefs_dir: brief::default_briefs_dir()?,
        inspect_addr: Some(state.config.inspect_listen_addr.clone()),
        mcp_server: super::mcp_preflight::find_server(),
        backend_reason: None,
    })
}

pub async fn kill_for_server(state: &BaseState, session: Uuid) -> Result<KillReport, LaunchError> {
    let backend = configured_backend(state).await?;
    let runner = build_runner(state, backend.as_ref()).await?;
    kill::execute(&runner, session).await
}

pub async fn doctor_for_server(state: &BaseState) -> Result<DoctorReport, LaunchError> {
    let backend = configured_backend(state).await?;
    let runner = build_runner(state, backend.as_ref()).await?;
    let env = super::doctor_env(&state.config)?;
    Ok(doctor::execute(&runner, &env).await)
}
