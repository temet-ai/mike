use crate::store::persistence::web_repo::WebPersist as _;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use super::RepairContext;
use super::port::{LiveBuild, RepairEngineer, RepairJob, RepairRun};
use super::runner::ProcessRunner;
use super::scratch_tests::{CHECK_NAME, Scratch, ScratchGh, write};
use super::worktree::RepoLayout;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db, seed_project};
use crate::store::persistence::wright::{FixBatchRow, ProblemRow, RepairRow};
use crate::wright::{CheckOutcome, CheckRun, Finding};

pub(super) use super::scratch_tests::{BUGGY, FIXED, PrState, sh};

type Files = Vec<(String, String)>;

fn owned(files: &[(&str, &str)]) -> Files {
    files
        .iter()
        .map(|(p, t)| ((*p).to_owned(), (*t).to_owned()))
        .collect()
}

pub(super) struct WritesFiles {
    pub(super) files: Files,
    pub(super) result: RepairRun,
    pub(super) steps: Mutex<VecDeque<(Files, RepairRun)>>,
    pub(super) jobs: Mutex<Vec<RepairJob>>,
}

impl WritesFiles {
    pub(super) fn new(files: &[(&str, &str)]) -> Self {
        Self {
            files: owned(files),
            result: RepairRun::default(),
            steps: Mutex::new(VecDeque::new()),
            jobs: Mutex::new(Vec::new()),
        }
    }

    pub(super) fn ending(mut self, result: RepairRun) -> Self {
        self.result = result;
        self
    }

    pub(super) fn then(self, files: &[(&str, &str)]) -> Self {
        self.then_ending(files, RepairRun::default())
    }

    pub(super) fn then_ending(self, files: &[(&str, &str)], result: RepairRun) -> Self {
        if let Ok(mut steps) = self.steps.lock() {
            steps.push_back((owned(files), result));
        }
        self
    }

    pub(super) fn jobs(&self) -> Vec<RepairJob> {
        self.jobs.lock().map(|j| j.clone()).unwrap_or_default()
    }

    fn next_step(&self) -> (Files, RepairRun) {
        self.steps
            .lock()
            .ok()
            .and_then(|mut steps| steps.pop_front())
            .unwrap_or_else(|| (self.files.clone(), self.result.clone()))
    }
}

impl RepairEngineer for WritesFiles {
    async fn run_in_worktree(&self, job: &RepairJob) -> RepairRun {
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.push(job.clone());
        }
        let (files, result) = self.next_step();
        for (path, text) in &files {
            if write(&job.worktree.join(path), text).is_err() {
                return RepairRun {
                    failure: Some(format!("could not write {path}")),
                    ..RepairRun::default()
                };
            }
        }
        result
    }
}

pub(super) struct FixedLive(pub(super) Mutex<Option<String>>);

impl FixedLive {
    pub(super) fn set(&self, sha: Option<String>) {
        if let Ok(mut live) = self.0.lock() {
            *live = sha;
        }
    }
}

impl LiveBuild for FixedLive {
    fn commit(&self) -> Option<String> {
        self.0.lock().ok().and_then(|live| live.clone())
    }
}

pub(super) type Ctx = RepairContext<ScratchGh, WritesFiles, FixedLive>;

pub(super) struct Bench {
    pub(super) db: TestDbHandle,
    pub(super) ctx: Ctx,
    pub(super) scratch: Arc<Scratch>,
}

impl Bench {
    pub(super) async fn boot(engineer: WritesFiles) -> anyhow::Result<Self> {
        let db = acquire_ephemeral_db()
            .await?
            .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
        let persist = PersistHandle::connect(&db.database_url).await?;
        let scratch = Arc::new(Scratch::create()?);
        let repo = RepoLayout::discover(&ProcessRunner, &scratch.root).await?;
        let ctx = RepairContext {
            persist,
            runner: ScratchGh::new(scratch.clone()),
            engineer,
            live: FixedLive(Mutex::new(None)),
            repo,
            machine: None,
            machine_name: Some("omit".to_owned()),
            shutdown: tokio_util::sync::CancellationToken::new(),
        };
        Ok(Self { db, ctx, scratch })
    }

    pub(super) fn restarted(&self, engineer: WritesFiles) -> Ctx {
        RepairContext {
            persist: self.ctx.persist.clone(),
            runner: ScratchGh::new(self.scratch.clone()),
            engineer,
            live: FixedLive(Mutex::new(None)),
            repo: self.ctx.repo.clone(),
            machine: None,
            machine_name: Some("omit".to_owned()),
            shutdown: tokio_util::sync::CancellationToken::new(),
        }
    }

    pub(super) async fn problems(&self, resources: &[&str]) -> anyhow::Result<Vec<i64>> {
        let findings = resources
            .iter()
            .map(|resource| Finding {
                scope: "capture".to_owned(),
                resource: (*resource).to_owned(),
                evidence: json!({"failed_requests": 7}),
            })
            .collect();
        self.ctx
            .persist
            .web()
            .record_wright_run(&CheckRun {
                check: CHECK_NAME,
                source: "capture_db",
                cadence: "regular",
                started_at: Utc::now(),
                outcome: &CheckOutcome::Complete(findings),
                machine: None,
            })
            .await?;
        let names: Vec<String> = resources.iter().map(|r| (*r).to_owned()).collect();
        Ok(sqlx::query_scalar(
            "SELECT p.id FROM unnest($1::text[]) WITH ORDINALITY AS r(resource, n)
               JOIN wright.problems p ON p.resource = r.resource AND p.check_name = $2
              ORDER BY r.n",
        )
        .bind(&names)
        .bind(CHECK_NAME)
        .fetch_all(self.ctx.persist.pool())
        .await?)
    }

    pub(super) async fn finding(
        &self,
        problem: i64,
        kind: &str,
        established: bool,
        part: Option<&str>,
    ) -> anyhow::Result<i64> {
        Ok(sqlx::query_scalar(
            "WITH run AS (
                 INSERT INTO wright.investigations (mode, problem_id, session_uuid, state, host,
                                                    deadline_at, ended_at)
                 VALUES ('investigate', $1, gen_random_uuid(), 'finished', 'test', now(), now())
                 RETURNING id
             )
             INSERT INTO wright.findings (investigation_id, problem_id, symptom, suspected_cause,
                                          confirmed_cause, cause_established, owning_part, kind,
                                          remedy, evidence)
             SELECT run.id, $1, 'requests fail', 'calc subtracts',
                    CASE WHEN $2 THEN 'add uses minus' END, $2, $3, $4, 'use plus',
                    '[{\"kind\": \"problem\", \"ref\": \"fixture\"}]'::jsonb
               FROM run
             RETURNING id",
        )
        .bind(problem)
        .bind(established)
        .bind(part)
        .bind(kind)
        .fetch_one(self.ctx.persist.pool())
        .await?)
    }

    pub(super) async fn defect_in(&self, part: &str) -> anyhow::Result<(i64, i64)> {
        let resource = format!("calc {}", Uuid::new_v4());
        let problems = self.problems(&[resource.as_str()]).await?;
        let problem = *problems
            .first()
            .ok_or_else(|| anyhow::anyhow!("the check run opened no problem"))?;
        let finding = self
            .finding(problem, "code_defect", true, Some(part))
            .await?;
        Ok((problem, finding))
    }

    pub(super) async fn defects(&self, parts: &[&str]) -> anyhow::Result<Vec<i64>> {
        let resources: Vec<String> = parts
            .iter()
            .map(|_| format!("calc {}", Uuid::new_v4()))
            .collect();
        let names: Vec<&str> = resources.iter().map(String::as_str).collect();
        let problems = self.problems(&names).await?;
        let mut findings = Vec::new();
        for (problem, part) in problems.into_iter().zip(parts) {
            findings.push(
                self.finding(problem, "code_defect", true, Some(part))
                    .await?,
            );
        }
        Ok(findings)
    }

    pub(super) async fn defect(&self) -> anyhow::Result<(i64, i64)> {
        self.defect_in("src/calc.sh").await
    }

    pub(super) async fn age(&self, problem: i64, hours: i32) -> anyhow::Result<()> {
        sqlx::query(
            "WITH p AS (
                 UPDATE wright.problems
                    SET healthy_at = healthy_at - make_interval(hours => $2)
                  WHERE id = $1 RETURNING id
             ), e AS (
                 UPDATE wright.episodes
                    SET opened_at = opened_at - make_interval(hours => $2)
                  WHERE problem_id = $1 AND closed_at IS NOT NULL RETURNING id
             )
             UPDATE wright.repairs
                SET created_at = created_at - make_interval(hours => $2),
                    prepared_at = prepared_at - make_interval(hours => $2),
                    landed_at = landed_at - make_interval(hours => $2),
                    deployed_at = deployed_at - make_interval(hours => $2)
              WHERE problem_id = $1",
        )
        .bind(problem)
        .bind(hours)
        .execute(self.ctx.persist.pool())
        .await?;
        Ok(())
    }

    pub(super) async fn problem_state(&self, id: i64) -> anyhow::Result<ProblemRow> {
        self.ctx
            .persist
            .web()
            .wright_problem(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("problem {id} vanished"))
    }

    pub(super) async fn repair_of(&self, finding: i64) -> anyhow::Result<RepairRow> {
        let id: i64 = sqlx::query_scalar("SELECT id FROM wright.repairs WHERE finding_id = $1")
            .bind(finding)
            .fetch_one(self.ctx.persist.pool())
            .await?;
        self.ctx
            .persist
            .web()
            .wright_repair(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("repair {id} vanished"))
    }

    pub(super) fn batch_dir(&self) -> PathBuf {
        self.ctx.repo.batch_dir(Utc::now().date_naive())
    }

    pub(super) fn batch_branch(&self) -> String {
        super::worktree::batch_branch(Utc::now().date_naive())
    }

    pub(super) async fn batches(&self) -> anyhow::Result<Vec<FixBatchRow>> {
        let mut rows = self
            .ctx
            .persist
            .web()
            .list_wright_fix_batches(None, 50)
            .await?;
        rows.reverse();
        Ok(rows)
    }

    pub(super) async fn open_batch(&self) -> anyhow::Result<FixBatchRow> {
        let row = self
            .ctx
            .persist
            .web()
            .ensure_wright_fix_batch(
                Utc::now().date_naive(),
                &self.batch_branch(),
                &self.batch_dir().display().to_string(),
                None,
            )
            .await?;
        super::worktree::open_batch(
            &self.ctx.runner,
            &self.ctx.repo,
            &self.batch_dir(),
            &row.branch,
            &row.branch,
        )
        .await
        .map_err(anyhow::Error::msg)?;
        Ok(row)
    }

    pub(super) async fn only_batch(&self) -> anyhow::Result<FixBatchRow> {
        let mut rows = self.batches().await?;
        anyhow::ensure!(rows.len() == 1, "expected one batch, found {}", rows.len());
        rows.pop().ok_or_else(|| anyhow::anyhow!("no batch"))
    }

    pub(super) async fn backdate(&self, batch: i64) -> anyhow::Result<()> {
        let row = self
            .ctx
            .persist
            .web()
            .wright_fix_batch(batch)
            .await?
            .ok_or_else(|| anyhow::anyhow!("batch {batch} vanished"))?;
        let day = row
            .day
            .pred_opt()
            .ok_or_else(|| anyhow::anyhow!("no day before {}", row.day))?;
        let branch = super::worktree::batch_branch(day);
        let path = self.ctx.repo.batch_dir(day).display().to_string();
        sh(&self.scratch.root, &["branch", "-m", &row.branch, &branch])?;
        sh(
            &self.scratch.root,
            &["worktree", "move", &row.worktree, &path],
        )?;
        sqlx::query(
            "UPDATE wright.fix_batches
                SET day = $2, branch = $3, branches = ARRAY[$3::text], worktree = $4
              WHERE id = $1",
        )
        .bind(batch)
        .bind(day)
        .bind(&branch)
        .bind(&path)
        .execute(self.ctx.persist.pool())
        .await?;
        Ok(())
    }

    pub(super) async fn repairs_in(&self, batch: i64) -> anyhow::Result<Vec<RepairRow>> {
        let ids: Vec<i64> =
            sqlx::query_scalar("SELECT id FROM wright.repairs WHERE batch_id = $1 ORDER BY id")
                .bind(batch)
                .fetch_all(self.ctx.persist.pool())
                .await?;
        let mut rows = Vec::new();
        for id in ids {
            let row = self
                .ctx
                .persist
                .web()
                .wright_repair(id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("repair {id} vanished"))?;
            rows.push(row);
        }
        Ok(rows)
    }

    pub(super) async fn handed_off_followup(&self, number: i32) -> anyhow::Result<i64> {
        Ok(sqlx::query_scalar(
            "INSERT INTO wright.followups (kind, owning_part, title, body, state, issue_number,
                                          fixer_session, fixer_machine, filed_at, handed_off_at)
             VALUES ('code_defect', $2, 'a fixer works on it', 'body', 'handed_off', $1,
                     gen_random_uuid(), 'omit', now(), now())
             RETURNING id",
        )
        .bind(number)
        .bind(format!("part-{number}"))
        .fetch_one(self.ctx.persist.pool())
        .await?)
    }

    pub(super) async fn operator_session(&self) -> anyhow::Result<Uuid> {
        let pool = self.ctx.persist.pool();
        let project = seed_project(pool, "/wright/repair").await?;
        let session = Uuid::new_v4();
        sqlx::query(
            "WITH s AS (INSERT INTO sessions.sessions (session_uuid, project_id) VALUES ($1, $2) RETURNING id),
                  g AS (INSERT INTO sessions.session_goals (session_id, goal, role)
                        SELECT id, 'wright', 'wright' FROM s)
             INSERT INTO workspace.session_registrations (session_id, project_id, cwd, lease_expires_at)
             SELECT id, $2, '/wright/repair', now() + interval '10 minutes' FROM s",
        )
        .bind(session)
        .bind(project)
        .execute(pool)
        .await?;
        Ok(session)
    }

    pub(super) async fn mail_to(&self, session: Uuid) -> anyhow::Result<Vec<String>> {
        Ok(sqlx::query_scalar(
            "SELECT m.body FROM mailbox.messages m JOIN sessions.sessions s ON s.id = m.to_session_id
              WHERE s.session_uuid = $1 ORDER BY m.id",
        )
        .bind(session)
        .fetch_all(self.ctx.persist.pool())
        .await?)
    }

    pub(super) async fn cleanup(self) -> anyhow::Result<()> {
        self.db.cleanup().await
    }
}
