use std::path::PathBuf;

use chrono::Utc;
use serde::Serialize;
use uuid::Uuid;

use super::{forks, logs};
use crate::store::persistence::wright::RecordedRun;
use crate::store::persistence::{PersistError, PersistHandle};
use crate::wright::{Cadence, CheckOutcome, CheckRun, Source};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Probe {
    Sql(&'static str),
    Logs,
    ColdForks,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub(crate) struct CheckSpec {
    pub(crate) name: &'static str,
    pub(crate) source: Source,
    pub(crate) cadence: Cadence,
    pub(crate) expects: &'static str,
    #[serde(skip)]
    pub(crate) probe: Probe,
}

pub(crate) const CHECKS: &[CheckSpec] = &[
    CheckSpec {
        name: "log_errors",
        source: Source::Logs,
        cadence: Cadence::Regular,
        expects: "No ERROR log line of one target and shape repeats 3 times within an hour, and nothing panics",
        probe: Probe::Logs,
    },
    CheckSpec {
        name: "request_failures",
        source: Source::CaptureDb,
        cadence: Cadence::Event,
        expects: "No request error of one shape (wire error, aborted stream, upstream failure) recurs 5 times within an hour",
        probe: Probe::Sql(include_str!("sql/request_failures.sql")),
    },
    CheckSpec {
        name: "undelivered_mail",
        source: Source::CaptureDb,
        cadence: Cadence::Regular,
        expects: "No session that kept working has 3 or more messages that were never sent, pushed or dead-lettered after 30 minutes",
        probe: Probe::Sql(include_str!("sql/undelivered_mail.sql")),
    },
    CheckSpec {
        name: "mcp_tool_errors",
        source: Source::McpTools,
        cadence: Cadence::Regular,
        expects: "No mike MCP tool fails the same way 20 times a day",
        probe: Probe::Sql(include_str!("sql/mcp_tool_errors.sql")),
    },
    CheckSpec {
        name: "page_client_errors",
        source: Source::Page,
        cadence: Cadence::Regular,
        expects: "No page route reports the same load, console, API or unhandled error 3 times a day",
        probe: Probe::Sql(include_str!("sql/page_client_errors.sql")),
    },
    CheckSpec {
        name: "hook_errors",
        source: Source::Hooks,
        cadence: Cadence::Regular,
        expects: "No hook fails or refuses for the same reason 10 times a day",
        probe: Probe::Sql(include_str!("sql/hook_errors.sql")),
    },
    CheckSpec {
        name: "llm_role_errors",
        source: Source::Workers,
        cadence: Cadence::Regular,
        expects: "No LLM role fails half or more of at least 10 calls a day",
        probe: Probe::Sql(include_str!("sql/llm_role_errors.sql")),
    },
    CheckSpec {
        name: "session_tool_errors",
        source: Source::Sessions,
        cadence: Cadence::Regular,
        expects: "No tool error of one shape hits 3 or more sessions 10 times a day",
        probe: Probe::Sql(include_str!("sql/session_tool_errors.sql")),
    },
    CheckSpec {
        name: "scheduled_job_failures",
        source: Source::Workers,
        cadence: Cadence::Daily,
        expects: "No scheduled job's latest run on a machine is its second failure in a row",
        probe: Probe::Sql(include_str!("sql/scheduled_job_failures.sql")),
    },
    CheckSpec {
        name: "cold_forks",
        source: Source::CaptureDb,
        cadence: Cadence::Daily,
        expects: "No fork of a warm template starts without reading the template's prompt cache, and each cold fork names its cause",
        probe: Probe::ColdForks,
    },
];

pub(crate) fn find(name: &str) -> Option<&'static CheckSpec> {
    CHECKS.iter().find(|spec| spec.name == name)
}

#[derive(Debug, Clone)]
pub(crate) struct WatchContext {
    pub(crate) persist: PersistHandle,
    pub(crate) log_dir: Option<PathBuf>,
    pub(crate) machine: Option<Uuid>,
}

pub(crate) async fn probe(spec: &CheckSpec, ctx: &WatchContext) -> CheckOutcome {
    let pool = ctx.persist.pool();
    let result = match spec.probe {
        Probe::Sql(sql) => super::bounded::findings(pool, spec.name, sql).await,
        Probe::Logs => logs::check(ctx.log_dir.as_deref(), Utc::now()).await,
        Probe::ColdForks => forks::check(pool, spec.name).await,
    };
    match result {
        Ok(findings) => CheckOutcome::Complete(findings),
        Err(reason) => CheckOutcome::Unknown(reason),
    }
}

pub(crate) async fn run_check(
    spec: &CheckSpec,
    ctx: &WatchContext,
) -> Result<(CheckOutcome, RecordedRun), PersistError> {
    let started_at = Utc::now();
    let outcome = probe(spec, ctx).await;
    let recorded = ctx
        .persist
        .record_wright_run(&CheckRun {
            check: spec.name,
            source: spec.source,
            cadence: spec.cadence,
            started_at,
            outcome: &outcome,
            machine: ctx.machine,
        })
        .await?;
    Ok((outcome, recorded))
}

pub(crate) fn due(cadences: &[Cadence]) -> impl Iterator<Item = &'static CheckSpec> + '_ {
    CHECKS
        .iter()
        .filter(move |spec| cadences.contains(&spec.cadence))
}
