use std::cell::RefCell;
use std::time::Duration;

use super::commands::ensure_allowed;
use super::endpoints::Endpoints;
use super::environment::{self, Inputs};
use super::error::ServiceError;
use super::exec::Context;
use super::files::Presence;
use super::guard::{address_answers, refuse_dev_stack};
use super::health::{self, Timing};
use super::manager::Manager;
use super::spec::ServiceKind;
use super::status::{self, UnitFacts};
use super::{install, restart};
use crate::config::ProxyConfig;
use crate::db_provision::{Runner, SystemRunner};

const RETRY_PAUSE: Duration = Duration::from_millis(500);

#[derive(Debug, Clone)]
pub(crate) struct Observation {
    pub(crate) manager_name: &'static str,
    pub(crate) unit_lines: Vec<String>,
    pub(crate) installed: bool,
    pub(crate) running: bool,
    pub(crate) dev_stack: Option<String>,
}

pub(crate) async fn observe_with<R: Runner>(runner: &R, manager: &Manager) -> Observation {
    let units = status::collect(runner, manager).await;
    let dev_stack = match refuse_dev_stack(runner, manager.platform).await {
        Err(ServiceError::DevStackActive { units }) => Some(units),
        _ => None,
    };
    Observation {
        manager_name: manager.platform.manager_name(),
        unit_lines: units.iter().map(UnitFacts::describe).collect(),
        installed: ServiceKind::ALL.iter().all(|kind| {
            units
                .iter()
                .any(|u| u.kind == *kind && u.presence == Presence::Ours)
        }),
        running: ServiceKind::ALL
            .iter()
            .all(|kind| units.iter().any(|u| u.kind == *kind && u.is_up())),
        dev_stack,
    }
}

pub(crate) async fn observe_for_setup() -> Option<Observation> {
    let manager = Manager::from_process().ok()?;
    Some(observe_with(&SystemRunner, &manager).await)
}

#[derive(Debug)]
pub(crate) struct Collected {
    pub(crate) lines: Vec<String>,
    pub(crate) error: Option<String>,
}

pub(crate) async fn install_for_setup(config: &ProxyConfig) -> Collected {
    collect(config, false).await
}

pub(crate) async fn restart_for_setup(config: &ProxyConfig) -> Collected {
    collect(config, true).await
}

async fn collect(config: &ProxyConfig, restart_only: bool) -> Collected {
    let lines = RefCell::new(Vec::new());
    let out = |line: &str| lines.borrow_mut().push(line.to_string());
    let result = perform(config, restart_only, &out).await;
    Collected {
        lines: lines.into_inner(),
        error: result.err().map(|e| e.to_string()),
    }
}

async fn perform(
    config: &ProxyConfig,
    restart_only: bool,
    out: &dyn Fn(&str),
) -> Result<(), ServiceError> {
    ensure_allowed(crate::sandbox::active(), "installing a background service")?;
    let manager = Manager::from_process()?;
    let endpoints = Endpoints::from_config(config);
    let ctx = Context {
        runner: &SystemRunner,
        manager: &manager,
        endpoints: &endpoints,
        listening: address_answers,
        pause: RETRY_PAUSE,
        out,
    };
    if restart_only {
        restart::restart(&ctx).await?;
    } else {
        let inputs = Inputs::from_process()?;
        let state_dir =
            crate::user_config::state_dir().map_err(|e| ServiceError::Config(e.to_string()))?;
        install::install(&ctx, &environment::specs(&inputs, &state_dir)).await?;
    }
    out("Checking that the services answer:");
    health::verify(&endpoints, Timing::AFTER_START, out).await
}
