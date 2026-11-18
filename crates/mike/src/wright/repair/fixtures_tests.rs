use crate::store::persistence::web_repo::WebPersist as _;
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
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db};
use crate::store::persistence::wright::{ProblemRow, RepairRow};
use crate::wright::{CheckOutcome, CheckRun, Finding};

pub(super) use super::scratch_tests::{BUGGY, FIXED, PrState, sh};

pub(super) struct WritesFiles {
    pub(super) files: Vec<(String, String)>,
    pub(super) result: RepairRun,
    pub(super) jobs: Mutex<Vec<RepairJob>>,
}

impl WritesFiles {
    pub(super) fn new(files: &[(&str, &str)]) -> Self {
        Self {
            files: files
                .iter()
                .map(|(p, t)| ((*p).to_owned(), (*t).to_owned()))
                .collect(),
            result: RepairRun::default(),
            jobs: Mutex::new(Vec::new()),
        }
    }

    pub(super) fn ending(mut self, result: RepairRun) -> Self {
        self.result = result;
        self
    }

    pub(super) fn jobs(&self) -> Vec<RepairJob> {
        self.jobs.lock().map(|j| j.clone()).unwrap_or_default()
    }
}

impl RepairEngineer for WritesFiles {
    async fn run_in_worktree(&self, job: &RepairJob) -> RepairRun {
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.push(job.clone());
        }
        for (path, text) in &self.files {
            if write(&job.worktree.join(path), text).is_err() {
                return RepairRun {
                    failure: Some(format!("could not write {path}")),
                    ..RepairRun::default()
                };
            }
        }
        self.result.clone()
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
            shutdown: tokio_util::sync::CancellationToken::new(),
        };
        Ok(Self { db, ctx, scratch })
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

    pub(super) async fn defect(&self) -> anyhow::Result<(i64, i64)> {
        let resource = format!("calc {}", Uuid::new_v4());
        let problems = self.problems(&[resource.as_str()]).await?;
        let problem = *problems
            .first()
            .ok_or_else(|| anyhow::anyhow!("the check run opened no problem"))?;
        let finding = self
            .finding(problem, "code_defect", true, Some("src/calc.sh"))
            .await?;
        Ok((problem, finding))
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

    pub(super) fn worktree(&self, finding: i64) -> PathBuf {
        self.ctx
            .repo
            .worktrees()
            .join(format!("wright-repair-{finding}"))
    }

    pub(super) async fn cleanup(self) -> anyhow::Result<()> {
        self.db.cleanup().await
    }
}
