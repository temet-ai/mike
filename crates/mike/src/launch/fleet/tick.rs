use std::future::Future;
use std::pin::Pin;

use super::definition::FleetDefinition;
use super::status::FleetStatus;
use crate::config::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::FleetRow;

pub(super) type StepFuture<'a> = Pin<Box<dyn Future<Output = ()> + Send + 'a>>;

pub(super) struct TickCtx<'a> {
    pub(super) state: &'a AppState,
    pub(super) persist: &'a PersistHandle,
    pub(super) row: &'a FleetRow,
    pub(super) status: &'a FleetStatus,
    pub(super) def: &'a FleetDefinition,
    pub(super) run_id: &'a str,
}

pub(super) struct Step {
    pub(super) name: &'static str,
    pub(super) run: for<'a> fn(&'a TickCtx<'a>) -> StepFuture<'a>,
}

pub(super) const STEPS: &[Step] = &[
    Step {
        name: "staff",
        run: staff_step,
    },
    Step {
        name: "supervise",
        run: supervise_step,
    },
    Step {
        name: "settle_checks",
        run: settle_checks_step,
    },
    Step {
        name: "shared_tree",
        run: shared_tree_step,
    },
    Step {
        name: "judge",
        run: judge_step,
    },
    Step {
        name: "problems",
        run: problems_step,
    },
    Step {
        name: "introduce",
        run: introduce_step,
    },
    Step {
        name: "wake",
        run: wake_step,
    },
    Step {
        name: "tell",
        run: tell_step,
    },
    Step {
        name: "escalate",
        run: escalate_step,
    },
    Step {
        name: "close",
        run: close_step,
    },
];

pub(super) fn should_close(state: &str, accepted: bool, control_seated: bool) -> bool {
    state == super::shutdown::RUNNING && accepted && !control_seated
}

pub(super) async fn run_steps(ctx: &TickCtx<'_>) {
    for step in STEPS {
        tracing::trace!(step = step.name, run = ctx.run_id, "fleet tick step");
        (step.run)(ctx).await;
    }
}

fn staff_step<'a>(ctx: &'a TickCtx<'a>) -> StepFuture<'a> {
    Box::pin(super::staff::staff(
        ctx.state,
        ctx.persist,
        ctx.row,
        ctx.status,
    ))
}

fn supervise_step<'a>(ctx: &'a TickCtx<'a>) -> StepFuture<'a> {
    Box::pin(async move {
        super::supervise::pass(ctx.state, ctx.persist, ctx.row, ctx.def, ctx.status).await;
        super::succession::pass(ctx.state, ctx.persist, ctx.row, ctx.def, ctx.status).await;
    })
}

fn settle_checks_step<'a>(ctx: &'a TickCtx<'a>) -> StepFuture<'a> {
    Box::pin(super::check_run::events::settle_interrupted(
        ctx.persist,
        ctx.run_id,
    ))
}

fn shared_tree_step<'a>(ctx: &'a TickCtx<'a>) -> StepFuture<'a> {
    Box::pin(super::shared_tree::pass(
        ctx.state,
        ctx.persist,
        ctx.def,
        ctx.run_id,
    ))
}

fn judge_step<'a>(ctx: &'a TickCtx<'a>) -> StepFuture<'a> {
    Box::pin(judge(ctx))
}

async fn judge(ctx: &TickCtx<'_>) {
    let fleet = ctx.row.fleet_uuid.to_string();
    super::leaves::judge_reports(ctx.state, ctx.persist, &fleet, ctx.def, ctx.run_id).await;
}

fn problems_step<'a>(ctx: &'a TickCtx<'a>) -> StepFuture<'a> {
    Box::pin(super::leaves::pass(
        ctx.state,
        ctx.persist,
        ctx.def,
        ctx.run_id,
    ))
}

fn introduce_step<'a>(ctx: &'a TickCtx<'a>) -> StepFuture<'a> {
    Box::pin(super::introduce::introduce(ctx.persist, ctx.row))
}

fn wake_step<'a>(ctx: &'a TickCtx<'a>) -> StepFuture<'a> {
    Box::pin(super::wake::pass(
        ctx.state,
        ctx.persist,
        ctx.def,
        ctx.run_id,
    ))
}

fn tell_step<'a>(ctx: &'a TickCtx<'a>) -> StepFuture<'a> {
    Box::pin(super::tell::tell(ctx.persist, ctx.row, ctx.status))
}

fn escalate_step<'a>(ctx: &'a TickCtx<'a>) -> StepFuture<'a> {
    Box::pin(super::questions::escalate(ctx.persist, ctx.row))
}

fn close_step<'a>(ctx: &'a TickCtx<'a>) -> StepFuture<'a> {
    Box::pin(close(ctx))
}

async fn close(ctx: &TickCtx<'_>) {
    let seated = super::seated::Seated::of_run(ctx.persist, ctx.run_id, ctx.def).await;
    if !should_close(
        &ctx.row.state,
        ctx.status.accepted,
        seated.control_seated(ctx.def),
    ) {
        return;
    }
    stop_and_audit(ctx).await;
}

async fn stop_and_audit(ctx: &TickCtx<'_>) {
    match super::shutdown::stop(ctx.state, ctx.persist, ctx.row).await {
        Ok(report) => {
            log_closed(ctx.run_id, &report.state);
            super::rule_audit::start(ctx.state, ctx.persist, ctx.row, ctx.def, ctx.run_id).await;
        }
        Err(err) => log_close_failed(ctx.run_id, &err),
    }
}

fn log_closed(run: &str, state: &dyn std::fmt::Display) {
    tracing::info!(run, state = %state, "fleet: every task is accepted, the loop closed the fleet");
}

fn log_close_failed(run: &str, err: &dyn std::fmt::Display) {
    tracing::warn!(run, err = %err, "fleet: every task is accepted but the close failed");
}

#[cfg(test)]
#[path = "tick_tests.rs"]
mod tick_tests;
