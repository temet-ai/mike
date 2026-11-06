use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use super::definition::FleetDefinition;
use crate::config::AppState;
use crate::launch::ServerOpenRequest;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::FleetRow;
use crate::store::persistence::launch::{LaunchListFilter, LaunchRow, NewRun};

pub(super) const RUN_PREFIX: &str = "rule-audit-";
pub(super) const IDLE_MINUTES: i64 = 5;
const AGENT: &str = "fleet-rule-auditor";
const COMPLETE: &str = "complete";
const CLOSE_REASON: &str = "one-shot job finished its first turn and sat idle";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AuditLaunch {
    pub(super) source_run: String,
    pub(super) run_id: String,
    pub(super) session: Uuid,
    pub(super) folder: String,
    pub(super) prompt: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct OneShot {
    pub(super) launch_id: i64,
    pub(super) session: Uuid,
    pub(super) has_brief: bool,
    pub(super) last_status: Option<String>,
    pub(super) last_seen: Option<DateTime<Utc>>,
}

#[async_trait::async_trait]
pub(super) trait Effects: Send + Sync {
    async fn launch(&self, launch: &AuditLaunch) -> Result<(), String>;
    async fn kill(&self, session: Uuid) -> Result<(), String>;
    async fn record_closed(&self, launch_id: i64, reason: &str) -> Result<(), String>;
}

#[async_trait::async_trait]
pub(super) trait Ledger: Send + Sync {
    async fn already(&self, key: &str) -> bool;
    async fn mark(&self, key: &str);
}

pub(super) fn told_key(source_run: &str) -> String {
    format!("rule-audit:{source_run}")
}

pub(super) fn audit_run_id(source_run: &str) -> String {
    format!("{RUN_PREFIX}{source_run}")
}

pub(super) fn prompt(source_run: &str, session: Uuid) -> String {
    format!(
        "Fleet run {source_run} has closed. Audit it. Your brief's run id is {source_run}; your session is {session}. Follow the 'After a run' section of your agent file, then end your turn."
    )
}

pub(super) fn is_idle(shot: &OneShot, now: DateTime<Utc>) -> bool {
    let finished = shot
        .last_status
        .as_deref()
        .is_some_and(|status| status.eq_ignore_ascii_case(COMPLETE));
    let quiet = shot
        .last_seen
        .is_some_and(|seen| now - seen >= Duration::minutes(IDLE_MINUTES));
    shot.has_brief && finished && quiet
}

pub(super) async fn start_after_close(
    effects: &dyn Effects,
    ledger: &dyn Ledger,
    folder: &str,
    source_run: &str,
) -> bool {
    let key = told_key(source_run);
    if ledger.already(&key).await {
        return false;
    }
    let session = Uuid::new_v4();
    let launch = AuditLaunch {
        source_run: source_run.to_string(),
        run_id: audit_run_id(source_run),
        session,
        folder: folder.to_string(),
        prompt: prompt(source_run, session),
    };
    match effects.launch(&launch).await {
        Ok(()) => {
            ledger.mark(&key).await;
            true
        }
        Err(err) => {
            tracing::warn!(run = source_run, %err, "fleet: the rule audit did not start; retrying next tick");
            false
        }
    }
}

pub(super) async fn close_idle(
    effects: &dyn Effects,
    shots: &[OneShot],
    now: DateTime<Utc>,
) -> Vec<i64> {
    let mut closed = Vec::new();
    for shot in shots.iter().filter(|shot| is_idle(shot, now)) {
        closed.extend(close_one(effects, shot).await);
    }
    closed
}

async fn close_one(effects: &dyn Effects, shot: &OneShot) -> Option<i64> {
    if let Err(err) = effects.kill(shot.session).await {
        log_not_killed(shot, &err);
        return None;
    }
    let recorded = effects.record_closed(shot.launch_id, CLOSE_REASON).await;
    recorded
        .inspect_err(|err| log_not_recorded(shot, err))
        .ok()
        .map(|()| shot.launch_id)
}

fn log_not_killed(shot: &OneShot, err: &dyn std::fmt::Display) {
    tracing::warn!(session = %shot.session, err = %err, "fleet: could not close an idle one-shot launch");
}

fn log_not_recorded(shot: &OneShot, err: &dyn std::fmt::Display) {
    tracing::warn!(launch = shot.launch_id, err = %err, "fleet: closed a one-shot launch but could not record it");
}

struct Live<'a> {
    state: &'a AppState,
    persist: &'a PersistHandle,
    run: &'a str,
}

#[async_trait::async_trait]
impl Effects for Live<'_> {
    async fn launch(&self, launch: &AuditLaunch) -> Result<(), String> {
        self.persist
            .upsert_run(&NewRun {
                run_id: launch.run_id.clone(),
                project_path: Some(launch.folder.clone()),
                backend: self.state.config.launch.backend.clone(),
                workspace_ref: None,
                tab_ref: None,
            })
            .await
            .map_err(|err| err.to_string())?;
        let request = ServerOpenRequest {
            folder: launch.folder.clone(),
            claude_args: vec![
                "--session-id".to_string(),
                launch.session.to_string(),
                "--agent".to_string(),
                AGENT.to_string(),
                "--model".to_string(),
                "sonnet".to_string(),
            ],
            prompt: Some(launch.prompt.clone()),
            run: Some(launch.run_id.clone()),
            seat: Some("rule-audit".to_string()),
            session_name: Some(format!("rule-audit-{}", launch.source_run)),
            role: Some("rule-audit".to_string()),
            instructions: Some(
                "One-shot rule audit of a closed fleet run; end the turn when done.".to_string(),
            ),
            goal: Some(format!(
                "Audit the fleet text after run {}",
                launch.source_run
            )),
            template_id: None,
            worktree_ok: super::environment::repo_is_linked_worktree(&launch.folder).await,
            trust: false,
            proxy_env: true,
            remote_control: true,
            channels: true,
            backend: None,
            caller: Default::default(),
        };
        crate::launch::cli::open_unwatched(self.state.clone(), request)
            .await
            .map(|_| ())
            .map_err(|err| err.to_string())
    }

    async fn kill(&self, session: Uuid) -> Result<(), String> {
        crate::launch::cli::kill_for_server(self.state, session)
            .await
            .map(|_| ())
            .map_err(|err| err.to_string())
    }

    async fn record_closed(&self, launch_id: i64, reason: &str) -> Result<(), String> {
        self.persist
            .record_launch_closed(launch_id, reason, Some(true))
            .await
            .map_err(|err| err.to_string())
    }
}

#[async_trait::async_trait]
impl Ledger for Live<'_> {
    async fn already(&self, key: &str) -> bool {
        super::told::already(self.persist, self.run, key).await
    }

    async fn mark(&self, key: &str) {
        super::told::mark(self.persist, self.run, &[key.to_string()]).await;
    }
}

pub(super) async fn start(
    state: &AppState,
    persist: &PersistHandle,
    row: &FleetRow,
    def: &FleetDefinition,
    run_id: &str,
) {
    let live = Live {
        state,
        persist,
        run: run_id,
    };
    if start_after_close(&live, &live, &def.repo, run_id).await {
        tracing::info!(run = run_id, fleet = %row.slug, "fleet: started the rule audit for the closed run");
    }
}

fn shot_of(launch: &LaunchRow, status: Option<(Option<String>, DateTime<Utc>)>) -> Option<OneShot> {
    let session = launch.session_uuid?;
    let (last_status, last_seen) = match status {
        Some((status, seen)) => (status, Some(seen)),
        None => (None, None),
    };
    Some(OneShot {
        launch_id: launch.id,
        session,
        has_brief: launch.prompt_present || launch.brief_text.is_some(),
        last_status,
        last_seen,
    })
}

pub(super) async fn close_finished(state: &AppState, persist: &PersistHandle) {
    let launches = match persist
        .list_launches(&LaunchListFilter {
            run_id: None,
            open_only: true,
            backend: None,
            limit: i64::MAX,
        })
        .await
    {
        Ok(rows) => rows,
        Err(err) => {
            tracing::warn!(%err, "fleet: could not list open launches for the one-shot close");
            return;
        }
    };
    let audits: Vec<&LaunchRow> = launches
        .iter()
        .filter(|l| {
            l.opened
                && l.run_id
                    .as_deref()
                    .is_some_and(|r| r.starts_with(RUN_PREFIX))
        })
        .collect();
    let ids: Vec<Uuid> = audits.iter().filter_map(|l| l.session_uuid).collect();
    let summaries = persist.list_sessions_by_ids(&ids).await.unwrap_or_default();
    let shots: Vec<OneShot> = audits
        .iter()
        .filter_map(|launch| {
            let summary = summaries
                .iter()
                .find(|s| {
                    Some(s.session_id.as_str())
                        == launch.session_uuid.map(|u| u.to_string()).as_deref()
                })
                .map(|s| (s.last_status.clone(), s.last_seen));
            shot_of(launch, summary)
        })
        .collect();
    let live = Live {
        state,
        persist,
        run: "",
    };
    close_idle(&live, &shots, Utc::now()).await;
}

#[cfg(test)]
#[path = "rule_audit_tests.rs"]
mod rule_audit_tests;
