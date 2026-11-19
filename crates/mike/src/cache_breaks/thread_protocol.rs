use std::sync::LazyLock;
use std::time::{Duration, Instant};

use sqlx::PgPool;
use tokio::sync::Mutex;

use crate::state::AppState;
use crate::store::persistence::messages::delta_correction::{self, CorrectionState};
use crate::web_base::capability::{CapabilityRow, CapabilityState};

const ID: &str = "thread_protocol";
const LABEL: &str = "Claude Code thread protocol";
const CACHE_FOR: Duration = Duration::from_secs(300);
const MIN_REQUESTS: i64 = 50;
const UNRESOLVED_SHARE: f64 = 0.02;
const UNRESOLVED_FLOOR: i64 = 3;
const LINK_SHARE_DROP: f64 = 0.5;
const REMEDY: &str = "Claude Code changed how it sends conversations. Compare a recent request body (get_request_body) with crates/mike-base/src/store/models/thread_link.rs and crates/mike-base/src/extract/request/turn_origin_classify.rs, then fix capture before the noise reaches requests.deltas.";

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub(crate) struct Readings {
    pub(crate) requests: i64,
    pub(crate) linked: i64,
    pub(crate) continues: i64,
    pub(crate) unresolved: i64,
    pub(crate) unknown_kind: i64,
    pub(crate) resends: i64,
    pub(crate) unseen_continuations: i64,
    pub(crate) label_shaped: i64,
    pub(crate) day_requests: i64,
    pub(crate) day_linked: i64,
}

static CACHED: LazyLock<Mutex<Option<(Instant, Readings)>>> = LazyLock::new(|| Mutex::new(None));

fn row(state: CapabilityState, detail: String, fix: String) -> CapabilityRow {
    CapabilityRow {
        id: ID,
        label: LABEL,
        state,
        detail,
        fix,
    }
}

fn share(part: i64, whole: i64) -> f64 {
    if whole <= 0 {
        return 0.0;
    }
    part as f64 / whole as f64
}

pub(crate) fn problems(r: &Readings) -> Vec<String> {
    let mut found = Vec::new();
    if r.unknown_kind > 0 {
        found.push(format!(
            "{} request(s) carried a thread type mike does not know",
            r.unknown_kind
        ));
    }
    if r.unresolved >= UNRESOLVED_FLOOR && share(r.unresolved, r.continues) > UNRESOLVED_SHARE {
        found.push(format!(
            "{} of {} continue requests named a response mike never captured",
            r.unresolved, r.continues
        ));
    }
    if r.unseen_continuations > 0 {
        found.push(format!(
            "{} tool-loop request(s) of one or two messages came without a thread, so their history is invisible",
            r.unseen_continuations
        ));
    }
    if r.label_shaped > 0 {
        found.push(format!(
            "{} agent label call(s) were captured as subagent turns",
            r.label_shaped
        ));
    }
    let hour_share = share(r.linked, r.requests);
    let day_share = share(r.day_linked, r.day_requests);
    if r.requests >= MIN_REQUESTS && hour_share < day_share * LINK_SHARE_DROP {
        found.push(format!(
            "thread-linked requests fell to {:.0}% this hour from {:.0}% over the day",
            hour_share * 100.0,
            day_share * 100.0
        ));
    }
    found
}

pub(crate) fn health_row(r: &Readings) -> CapabilityRow {
    let summary = format!(
        "Last hour: {} main and subagent requests, {} linked to the response they continue, {} continues, {} full re-sends. Last day: {:.0}% linked.",
        r.requests,
        r.linked,
        r.continues,
        r.resends,
        share(r.day_linked, r.day_requests) * 100.0
    );
    let found = problems(r);
    if found.is_empty() {
        return row(CapabilityState::Active, summary, String::new());
    }
    row(
        CapabilityState::Degraded,
        format!("{}. {summary}", found.join("; ")),
        REMEDY.to_owned(),
    )
}

pub(crate) async fn read(pool: &PgPool) -> Result<Readings, sqlx::Error> {
    sqlx::query_as(READINGS) // grader-allow: literal without parameters; no .sqlx entry is prepared on infra
        .fetch_one(pool)
        .await
}

pub(crate) async fn capability(state: &AppState) -> CapabilityRow {
    let Some(persist) = state.persist() else {
        return row(
            CapabilityState::Off,
            "Persistence is off, so the thread protocol is not watched".to_owned(),
            String::new(),
        );
    };
    let mut cached = CACHED.lock().await;
    if let Some((at, readings)) = cached.as_ref()
        && at.elapsed() < CACHE_FOR
    {
        return health_row(readings);
    }
    match read(persist.pool()).await {
        Ok(readings) => {
            *cached = Some((Instant::now(), readings));
            health_row(&readings)
        }
        Err(e) => row(
            CapabilityState::Degraded,
            format!("Could not read the thread protocol readings: {e}"),
            "Check that Postgres is reachable at the configured MIKE_DATABASE_URL".to_owned(),
        ),
    }
}

pub fn spawn_delta_correction(state: &AppState) {
    let Some(persist) = state.persist().cloned() else {
        return;
    };
    let pool = persist.pool().clone();
    state
        .tracker
        .spawn(delta_correction::run(pool, state.shutdown()));
}

pub(crate) fn correction_row(progress: Option<&CorrectionState>) -> CapabilityRow {
    let Some(p) = progress else {
        return row(
            CapabilityState::Off,
            "The thread-delta correction has not started".to_owned(),
            String::new(),
        );
    };
    let detail = if p.finished {
        format!(
            "Corrected {} requests captured before capture followed the thread link and deleted {} removed rows. Run `mike reclaim-space` to return the space.",
            p.requests_corrected, p.removed_rows_deleted
        )
    } else {
        format!(
            "Correcting requests captured before capture followed the thread link: {} done through request {} of {}, {} removed rows deleted so far.",
            p.requests_corrected, p.last_request_id, p.newest_request_id, p.removed_rows_deleted
        )
    };
    CapabilityRow {
        id: CORRECTION_ID,
        label: CORRECTION_LABEL,
        state: CapabilityState::Active,
        detail,
        fix: String::new(),
    }
}

pub(crate) async fn correction_capability(state: &AppState) -> CapabilityRow {
    let Some(persist) = state.persist() else {
        return correction_row(None);
    };
    match delta_correction::state(persist.pool()).await {
        Ok(progress) => correction_row(progress.as_ref()),
        Err(e) => CapabilityRow {
            id: CORRECTION_ID,
            label: CORRECTION_LABEL,
            state: CapabilityState::Degraded,
            detail: format!("Could not read the thread-delta correction progress: {e}"),
            fix: "Check that Postgres is reachable at the configured MIKE_DATABASE_URL".to_owned(),
        },
    }
}

const CORRECTION_ID: &str = "thread_delta_correction";
const CORRECTION_LABEL: &str = "Thread-delta correction";

const READINGS: &str = "WITH lanes AS MATERIALIZED (
        SELECT r.message_count, r.is_tool_loop, r.last_user_message, pr.code AS rule, tor.code AS origin,
               r.created_at >= now() - interval '1 hour' AS recent,
               CASE WHEN r.created_at >= now() - interval '1 hour' AND r.request_envelope IS NOT NULL
                    THEN convert_from(r.request_envelope, 'UTF8')::jsonb -> 'thread' END AS thread
          FROM requests.requests r
          JOIN requests.turn_origins tor ON tor.id = r.turn_origin_id
          LEFT JOIN requests.predecessor_rules pr ON pr.id = r.predecessor_rule_id
         WHERE r.created_at >= now() - interval '24 hours'
           AND tor.code IN ('primary', 'primary_first', 'subagent', 'workflow_subagent')
    )
    SELECT count(*) FILTER (WHERE recent) AS requests,
           count(*) FILTER (WHERE recent AND rule = 'thread_link') AS linked,
           count(*) FILTER (WHERE recent AND thread ->> 'type' = 'continue') AS continues,
           count(*) FILTER (WHERE recent AND thread ->> 'type' = 'continue' AND rule IS DISTINCT FROM 'thread_link') AS unresolved,
           count(*) FILTER (WHERE recent AND thread IS NOT NULL AND coalesce(thread ->> 'type', '') NOT IN ('create', 'continue')) AS unknown_kind,
           count(*) FILTER (WHERE recent AND rule IN ('thread_start', 'thread_restart') AND message_count >= 3) AS resends,
           count(*) FILTER (WHERE recent AND thread IS NULL AND is_tool_loop AND message_count <= 2) AS unseen_continuations,
           count(*) FILTER (WHERE recent AND origin = 'subagent' AND thread IS NULL AND message_count = 1 AND last_user_message LIKE '<task>%') AS label_shaped,
           count(*) FILTER (WHERE NOT recent) AS day_requests,
           count(*) FILTER (WHERE NOT recent AND rule = 'thread_link') AS day_linked
      FROM lanes";

#[cfg(test)]
#[path = "thread_protocol_tests.rs"]
mod tests;
