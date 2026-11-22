use crate::store::persistence::web_repo::WebPersist as _;
use std::io;
use std::path::PathBuf;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::context::{FollowupContext, Here};
use super::runner::{Cmd, CmdOutput, CommandRunner, Program};
use super::sessions::FixerSessions;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db};
use crate::store::persistence::wright::{
    FixBatchRow, Fixer, FollowupLimits, FollowupLimitsPatch, NewFollowup,
};
use crate::store::persistence::{PersistError, PersistHandle};

pub(super) const CHECK: &str = "undelivered_mail";
pub(super) const REPO: &str = "temet-ai/mike";
pub(super) const QUIET_PROBE: &str = "0.10 0.20 0.30 1/200 999\n8\n";
pub(super) const BUSY_PROBE: &str = "12.50 9.00 8.00 9/200 999\n8\n";

pub(super) fn ok(stdout: &str) -> CmdOutput {
    CmdOutput {
        code: Some(0),
        stdout: stdout.to_owned(),
        stderr: String::new(),
    }
}

pub(super) fn spawned(session: Uuid) -> String {
    format!(
        "{{\"backend\":\"herdr\",\"folder\":\"/srv/mike\",\"session\":\"{session}\",\"opened\":true,\"confirmed\":true}}\n"
    )
}

impl Cmd {
    pub(super) fn has_args(&self, wanted: &[&str]) -> bool {
        self.args
            .windows(wanted.len())
            .any(|window| window.iter().zip(wanted).all(|(a, b)| a == b))
    }
}

pub(super) struct FakeRunner {
    rules: Vec<(Program, String, CmdOutput)>,
    calls: Mutex<Vec<Cmd>>,
}

impl FakeRunner {
    pub(super) fn new() -> Self {
        Self {
            rules: Vec::new(),
            calls: Mutex::new(Vec::new()),
        }
    }

    pub(super) fn on(mut self, program: Program, needle: &str, out: CmdOutput) -> Self {
        self.rules.push((program, needle.to_owned(), out));
        self
    }

    pub(super) fn calls(&self) -> Vec<Cmd> {
        self.calls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    pub(super) fn called(&self, program: Program, needle: &str) -> Vec<Cmd> {
        self.calls()
            .into_iter()
            .filter(|cmd| cmd.program == program && cmd.line().contains(needle))
            .collect()
    }
}

impl CommandRunner for FakeRunner {
    async fn run(&self, cmd: &Cmd) -> io::Result<CmdOutput> {
        self.calls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(cmd.clone());
        let line = cmd.line();
        Ok(self
            .rules
            .iter()
            .find(|(program, needle, _)| *program == cmd.program && line.contains(needle.as_str()))
            .map_or_else(
                || CmdOutput {
                    code: Some(1),
                    stdout: String::new(),
                    stderr: format!("the fake runner has no answer for {line}"),
                },
                |(_, _, out)| out.clone(),
            ))
    }
}

pub(super) struct FakeSessions {
    pub(super) ended: Option<String>,
}

impl FixerSessions for FakeSessions {
    async fn ended(
        &self,
        _session: Uuid,
        _handed_off_at: DateTime<Utc>,
    ) -> Result<Option<String>, PersistError> {
        Ok(self.ended.clone())
    }
}

pub(super) fn remote_fixer() -> Fixer {
    Fixer {
        machine: "infra".to_owned(),
        host: Some("infra".to_owned()),
        folder: Some("/srv/mike".to_owned()),
    }
}

pub(super) struct Env {
    pub(super) db: TestDbHandle,
    pub(super) persist: PersistHandle,
    run: i64,
}

impl Env {
    pub(super) async fn boot() -> anyhow::Result<Self> {
        let db = acquire_ephemeral_db()
            .await?
            .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
        let persist = PersistHandle::connect(&db.database_url).await?;
        let run = sqlx::query_scalar(
            "INSERT INTO wright.check_runs (check_name, source, cadence, started_at, outcome)
             VALUES ($1, 'capture_db', 'regular', now(), 'fail') RETURNING id",
        )
        .bind(CHECK)
        .fetch_one(persist.pool())
        .await?;
        Ok(Self { db, persist, run })
    }

    pub(super) fn ctx<R, S>(&self, runner: R, sessions: S) -> FollowupContext<R, S> {
        FollowupContext {
            persist: self.persist.clone(),
            runner,
            sessions,
            here: Here {
                host: "testhost".to_owned(),
                checkout: None,
                loadavg: PathBuf::from("/nonexistent/loadavg"),
            },
        }
    }

    pub(super) async fn limits(
        &self,
        patch: FollowupLimitsPatch,
    ) -> anyhow::Result<FollowupLimits> {
        Ok(self
            .persist
            .web()
            .patch_wright_followup_limits(&patch)
            .await?)
    }

    pub(super) async fn followup(&self, kind: &str, part: &str) -> anyhow::Result<(i64, i64)> {
        let (problem, finding): (i64, i64) = sqlx::query_as(
            "WITH p AS (
                 INSERT INTO wright.problems (check_name, source, scope, resource, state)
                 VALUES ($1, 'capture_db', 'mail', gen_random_uuid()::text, 'open') RETURNING id
             ), e AS (
                 INSERT INTO wright.episodes (problem_id, opened_by_run, last_seen_run, evidence)
                 SELECT id, $2, $2, '{}'::jsonb FROM p RETURNING id, problem_id
             ), run AS (
                 INSERT INTO wright.investigations (mode, problem_id, episode_id, session_uuid, state,
                                                    host, deadline_at, ended_at)
                 SELECT 'investigate', problem_id, id, gen_random_uuid(), 'finished', 'test', now(), now()
                   FROM e RETURNING id, problem_id, episode_id
             ), f AS (
                 INSERT INTO wright.findings (investigation_id, problem_id, episode_id, symptom,
                                              suspected_cause, cause_established, kind, remedy, evidence)
                 SELECT id, problem_id, episode_id, 'launches die', 'unknown', false, 'unknown',
                        'look again', '[{\"kind\": \"problem\", \"ref\": \"fixture\"}]'::jsonb
                   FROM run RETURNING id, problem_id
             )
             SELECT problem_id, id FROM f",
        )
        .bind(CHECK)
        .bind(self.run)
        .fetch_one(self.persist.pool())
        .await?;
        let id = self
            .persist
            .web()
            .record_wright_followup(&NewFollowup {
                kind,
                check_name: Some(CHECK),
                owning_part: Some(part),
                title: "Launches are left open",
                body: "The launch ledger keeps rows for sessions that ended.",
                finding_id: finding,
                same_as: None,
            })
            .await?;
        Ok((id, problem))
    }

    pub(super) async fn filed(&self, part: &str, number: i32) -> anyhow::Result<i64> {
        let (id, _) = self.followup("hardening", part).await?;
        let url = format!("https://github.com/{REPO}/issues/{number}");
        anyhow::ensure!(
            self.persist
                .web()
                .mark_wright_followup_filed(id, number, &url)
                .await?
        );
        Ok(id)
    }

    pub(super) async fn handed(&self, part: &str, number: i32) -> anyhow::Result<i64> {
        let id = self.filed(part, number).await?;
        anyhow::ensure!(
            self.persist
                .web()
                .mark_wright_followup_handed_off(id, Uuid::new_v4(), "infra")
                .await?
        );
        Ok(id)
    }

    pub(super) async fn open_batch(&self) -> anyhow::Result<FixBatchRow> {
        let today = chrono::Utc::now().date_naive();
        Ok(self
            .persist
            .web()
            .ensure_wright_fix_batch(
                today,
                &format!("wright/fixes-{today}"),
                &format!("/srv/mike/.claude/worktrees/wright-fixes-{today}"),
                None,
            )
            .await?)
    }

    pub(super) async fn editing_repair(&self, batch: &FixBatchRow) -> anyhow::Result<i64> {
        let (_, problem) = self.followup("hardening", "repair-part").await?;
        Ok(sqlx::query_scalar(
            "INSERT INTO wright.repairs (finding_id, problem_id, owning_part, state, worktree,
                                         branch, branches, batch_id)
             SELECT f.id, f.problem_id, 'repair-part', 'editing', $2, $3, ARRAY[$3], $4
               FROM wright.findings f WHERE f.problem_id = $1
             RETURNING id",
        )
        .bind(problem)
        .bind(&batch.worktree)
        .bind(&batch.branch)
        .bind(batch.id)
        .fetch_one(self.persist.pool())
        .await?)
    }

    pub(super) async fn state_of(&self, id: i64) -> anyhow::Result<(String, i32, Option<i32>)> {
        Ok(sqlx::query_as(
            "SELECT state, attempts, issue_number FROM wright.followups WHERE id = $1",
        )
        .bind(id)
        .fetch_one(self.persist.pool())
        .await?)
    }
}
