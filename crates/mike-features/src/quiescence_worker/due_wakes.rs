use crate::store::persistence::steerer_repo::SteererPersist as _;
pub mod decide;

use chrono::Utc;

use super::wake_recheck::{self, Checked};
use crate::llm::decisions::point::Decider;

use decide::{WakeAction, WakeFacts, classify_reach, decide_due_wake};

use super::sweep::SweepContext;
use crate::dossier::steering_facts::SteeringFacts;
use crate::dossier::steering_facts::wait_backing::WaitBacking;
use crate::logged::ok_or_warn;
use crate::steerer::guard::policy::GuardSettings;
use crate::store::models::{ReminderSource, SystemReminder};
use crate::store::persistence::steering::promotion::{DueWakeRow, WakeFinal};

pub const DUE_BATCH: i64 = 50;
pub const DEMOTION_EVERY_TICKS: u64 = 60;

async fn backed_now(ctx: &SweepContext<'_>, row: &DueWakeRow) -> bool {
    if row.question.as_deref() != Some("unbacked_wait") {
        return false;
    }
    SteeringFacts::as_of(
        ctx.persist,
        row.session_id,
        row.evaluated_turn_id,
        Utc::now(),
    )
    .await
    .is_ok_and(|f| !matches!(f.wait_backing, WaitBacking::None | WaitBacking::Unknown))
}

fn reminder_for(row: &DueWakeRow) -> SystemReminder {
    SystemReminder {
        source: ReminderSource::Steerer {
            kind: "turn_guard".to_owned(),
            intervention_uuid: row.uuid.to_string(),
            advisory: false,
        },
        payload: crate::steerer::inject::build_steering_block_with(
            &row.payload_text,
            crate::steerer::inject::ReplyDemand::Required,
        ),
    }
}

async fn push(ctx: &SweepContext<'_>, row: &DueWakeRow) -> &'static str {
    let deps = ctx.push_deps();
    let reminder = reminder_for(row);
    if crate::proxy::remote_control::inject::try_push_reminder(&deps, row.session_uuid, &reminder)
        .await
    {
        "pushed"
    } else {
        "push_failed"
    }
}

async fn facts_for<'r>(ctx: &SweepContext<'_>, row: &'r DueWakeRow, cap: i64) -> WakeFacts<'r> {
    let deps = ctx.push_deps();
    let may_wake = deps
        .gate
        .admit(&row.session_uuid.to_string(), &reminder_for(row))
        .may_wake();
    WakeFacts {
        mode: row.mode.as_deref(),
        delivered: row.delivered,
        reverted: row.reverted,
        expired: row.expires_at <= Utc::now(),
        reach: classify_reach(row.turn_closed, row.moved_on, row.stream_open),
        unbacked_wait: row.question.as_deref() == Some("unbacked_wait"),
        backed_now: backed_now(ctx, row).await,
        pushed_last_hour: row.pushed_last_hour,
        cap,
        may_wake,
    }
}

async fn record_holdback_reach(ctx: &SweepContext<'_>) {
    let rows = ok_or_warn(
        ctx.persist.steerer().claim_reach_due(DUE_BATCH).await,
        "due_wakes.claim_reach_due",
    )
    .unwrap_or_default();
    let reach: Vec<(i64, &'static str)> = rows
        .iter()
        .map(|r| {
            (
                r.id,
                classify_reach(r.turn_closed, r.moved_on, r.stream_open).code(),
            )
        })
        .collect();
    ok_or_warn(
        ctx.persist.steerer().write_reach_at_due(&reach).await,
        "due_wakes.holdback_reach",
    );
}

#[derive(Default)]
struct DuePass {
    finals: Vec<WakeFinal>,
    back: Vec<i64>,
    reach: Vec<(i64, &'static str)>,
}

async fn claim_rows(ctx: &SweepContext<'_>) -> Vec<DueWakeRow> {
    let ids = ok_or_warn(
        ctx.persist.steerer().claim_due_wakes(DUE_BATCH).await,
        "due_wakes.claim",
    )
    .unwrap_or_default();
    if ids.is_empty() {
        return Vec::new();
    }
    ok_or_warn(
        ctx.persist.steerer().fetch_due_wake_context(&ids).await,
        "due_wakes.fetch_context",
    )
    .unwrap_or_default()
}

struct WakeJev {
    decider: Option<Decider>,
    failed: bool,
}

impl WakeJev {
    fn new(ctx: &SweepContext<'_>) -> Self {
        let decider = super::sweep::build_base_llm_client(ctx)
            .and_then(|base| crate::llm::key::keyed_client(&base, ctx.key_slot))
            .and_then(|keyed| {
                Decider::for_point(
                    &keyed,
                    ctx.persist,
                    ctx.toggles.load().decisions,
                    &ctx.settings.load(),
                    wake_recheck::POINT,
                )
            });
        Self {
            decider,
            failed: false,
        }
    }

    async fn check(&mut self, ctx: &SweepContext<'_>, row: &DueWakeRow) -> Option<Checked> {
        let decider = self.decider.as_ref().filter(|_| !self.failed)?;
        let checked = wake_recheck::check(decider, ctx.persist, row).await;
        if checked.is_none() && decider.applies() {
            self.failed = true;
        }
        checked
    }
}

async fn push_or_skip(ctx: &SweepContext<'_>, row: &DueWakeRow, jev: &mut WakeJev) -> WakeFinal {
    let checked = jev.check(ctx, row).await;
    let (Some(decider), Some(checked)) = (jev.decider.as_ref(), checked) else {
        return WakeFinal {
            id: row.id,
            state: push(ctx, row).await,
            expire: false,
        };
    };
    if checked.recheck.skip {
        wake_recheck::mark(decider, &checked, false).await;
        return WakeFinal {
            id: row.id,
            state: "backed",
            expire: true,
        };
    }
    let state = push(ctx, row).await;
    wake_recheck::mark(decider, &checked, state == "pushed").await;
    WakeFinal {
        id: row.id,
        state,
        expire: false,
    }
}

async fn decide_row(
    ctx: &SweepContext<'_>,
    row: &DueWakeRow,
    cap: i64,
    pass: &mut DuePass,
    jev: &mut WakeJev,
) {
    let facts = facts_for(ctx, row, cap).await;
    pass.reach.push((row.evaluation_id, facts.reach.code()));
    match decide_due_wake(&facts) {
        WakeAction::Resolve { state, expire } => pass.finals.push(WakeFinal {
            id: row.id,
            state,
            expire,
        }),
        WakeAction::BackToPending => pass.back.push(row.id),
        WakeAction::Push => pass.finals.push(push_or_skip(ctx, row, jev).await),
    }
}

async fn write_pass(ctx: &SweepContext<'_>, pass: &DuePass) {
    ok_or_warn(
        ctx.persist.steerer().finalise_wakes(&pass.finals).await,
        "due_wakes.finalise",
    );
    ok_or_warn(
        ctx.persist
            .steerer()
            .return_wakes_to_pending(&pass.back)
            .await,
        "due_wakes.return_to_pending",
    );
    ok_or_warn(
        ctx.persist.steerer().write_reach_at_due(&pass.reach).await,
        "due_wakes.reach_at_due",
    );
}

pub async fn run_due_wakes(ctx: &SweepContext<'_>, toggle_on: bool) {
    if !toggle_on {
        ok_or_warn(
            ctx.persist.steerer().disable_all_pending_wakes().await,
            "due_wakes.disable_pending",
        );
        return;
    }
    ok_or_warn(
        ctx.persist.steerer().recover_stale_wakes().await,
        "due_wakes.recover_stale",
    );
    record_holdback_reach(ctx).await;
    let rows = claim_rows(ctx).await;
    if !rows.is_empty() {
        process_rows(ctx, &rows).await;
    }
}

async fn process_rows(ctx: &SweepContext<'_>, rows: &[DueWakeRow]) {
    let cap = GuardSettings::load(ctx.persist)
        .await
        .wake_cap_per_session_hour;
    let mut pass = DuePass::default();
    let mut jev = WakeJev::new(ctx);
    for row in rows {
        decide_row(ctx, row, cap, &mut pass, &mut jev).await;
    }
    write_pass(ctx, &pass).await;
    tracing::debug!(claimed = rows.len(), "due wakes: pass finished");
}
