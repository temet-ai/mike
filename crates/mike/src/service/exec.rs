use std::time::Duration;

use super::endpoints::Endpoints;
use super::error::ServiceError;
use super::manager::Manager;
use super::spec::ServiceKind;
use super::state::Activity;
use crate::db_provision::{Call, Captured, Runner};

const BOOTSTRAP_ATTEMPTS: u32 = 4;

pub(crate) struct Context<'a, R: Runner> {
    pub(crate) runner: &'a R,
    pub(crate) manager: &'a Manager,
    pub(crate) endpoints: &'a Endpoints,
    pub(crate) listening: fn(&str) -> bool,
    pub(crate) pause: Duration,
    pub(crate) out: &'a dyn Fn(&str),
}

pub(crate) async fn run_loose<R: Runner>(
    runner: &R,
    call: &Call,
) -> Result<Captured, ServiceError> {
    runner
        .run(call)
        .await
        .map_err(|source| ServiceError::Spawn {
            program: call.program.clone(),
            source,
        })
}

pub(crate) async fn run_checked<R: Runner>(
    runner: &R,
    call: &Call,
) -> Result<Captured, ServiceError> {
    let captured = run_loose(runner, call).await?;
    if captured.success {
        Ok(captured)
    } else {
        Err(ServiceError::CommandFailed {
            command: call.shown(),
            detail: captured.detail(),
        })
    }
}

fn is_bootstrap(call: &Call) -> bool {
    call.program == "launchctl" && call.args.first().is_some_and(|a| a == "bootstrap")
}

pub(crate) async fn run_with_retry<R: Runner>(
    ctx: &Context<'_, R>,
    call: &Call,
) -> Result<(), ServiceError> {
    let attempts = if is_bootstrap(call) {
        BOOTSTRAP_ATTEMPTS
    } else {
        1
    };
    let mut last = run_checked(ctx.runner, call).await;
    for _ in 1..attempts {
        if last.is_ok() {
            break;
        }
        tokio::time::sleep(ctx.pause).await;
        last = run_checked(ctx.runner, call).await;
    }
    last.map(|_| ())
}

pub(crate) async fn activity<R: Runner>(
    runner: &R,
    manager: &Manager,
    kind: ServiceKind,
) -> Result<Activity, ServiceError> {
    let captured = run_loose(runner, &manager.state_call(kind)).await?;
    Ok(manager.parse_state(&captured))
}
