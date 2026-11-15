use crate::store::persistence::web_repo::WebPersist as _;
use uuid::Uuid;

use super::{Fixer, FollowupLimitsPatch, NewFollowup};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db};

const CHECK: &str = "orphan_launches";
const REMEDY: &str = "close_orphan_launches";

struct Env {
    db: TestDbHandle,
    persist: PersistHandle,
    run: i64,
}

impl Env {
    async fn boot() -> anyhow::Result<Self> {
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

    async fn problem(&self, check: &str, open_episode: bool) -> anyhow::Result<(i64, i64)> {
        let problem: i64 = sqlx::query_scalar(
            "INSERT INTO wright.problems (check_name, source, scope, resource, state)
             VALUES ($1, 'capture_db', 'launch', $2, 'open') RETURNING id",
        )
        .bind(check)
        .bind(Uuid::new_v4().to_string())
        .fetch_one(self.persist.pool())
        .await?;
        let episode: i64 = sqlx::query_scalar(
            "INSERT INTO wright.episodes (problem_id, closed_at, opened_by_run, last_seen_run, evidence)
             VALUES ($1, CASE WHEN $2 THEN NULL ELSE now() END, $3, $3, '{}'::jsonb) RETURNING id",
        )
        .bind(problem)
        .bind(open_episode)
        .bind(self.run)
        .fetch_one(self.persist.pool())
        .await?;
        Ok((problem, episode))
    }

    async fn finding(&self, problem: i64, episode: i64) -> anyhow::Result<i64> {
        Ok(sqlx::query_scalar(
            "WITH run AS (
                 INSERT INTO wright.investigations (mode, problem_id, episode_id, session_uuid, state,
                                                    host, deadline_at, ended_at)
                 VALUES ('investigate', $1, $2, gen_random_uuid(), 'finished', 'test', now(), now())
                 RETURNING id
             )
             INSERT INTO wright.findings (investigation_id, problem_id, episode_id, symptom,
                                          suspected_cause, cause_established, kind, remedy, evidence)
             SELECT run.id, $1, $2, 'launches die', 'unknown', false, 'unknown', 'look again',
                    '[{\"kind\": \"problem\", \"ref\": \"fixture\"}]'::jsonb
               FROM run
             RETURNING id",
        )
        .bind(problem)
        .bind(episode)
        .fetch_one(self.persist.pool())
        .await?)
    }

    async fn record(&self, kind: &str, part: Option<&str>, finding: i64) -> anyhow::Result<i64> {
        self.record_same(kind, part, finding, None).await
    }

    async fn record_same(
        &self,
        kind: &str,
        part: Option<&str>,
        finding: i64,
        same_as: Option<i64>,
    ) -> anyhow::Result<i64> {
        Ok(self
            .persist
            .web()
            .record_wright_followup(&NewFollowup {
                kind,
                check_name: Some(CHECK),
                owning_part: part,
                title: "title",
                body: "body",
                finding_id: finding,
                same_as,
            })
            .await?)
    }

    async fn verified_actions(&self, count: i32) -> anyhow::Result<(i64, i64)> {
        Ok(sqlx::query_as(
            "WITH p AS (
                 INSERT INTO wright.problems (check_name, source, scope, resource, state)
                 SELECT $1, 'capture_db', 'launch', gen_random_uuid()::text, 'open'
                   FROM generate_series(1, $2::int)
                 RETURNING id
             ), e AS (
                 INSERT INTO wright.episodes (problem_id, opened_by_run, last_seen_run, evidence)
                 SELECT id, $3, $3, '{}'::jsonb FROM p
                 RETURNING id, problem_id
             ), a AS (
                 INSERT INTO wright.actions (remedy, problem_id, episode_id, state, verified_at)
                 SELECT $4, problem_id, id, 'verified', now() - interval '1 day' FROM e
             )
             SELECT problem_id, id FROM e ORDER BY problem_id DESC LIMIT 1",
        )
        .bind(CHECK)
        .bind(count)
        .bind(self.run)
        .bind(REMEDY)
        .fetch_one(self.persist.pool())
        .await?)
    }

    async fn state_of(&self, id: i64) -> anyhow::Result<(String, i32)> {
        Ok(
            sqlx::query_as("SELECT state, attempts FROM wright.followups WHERE id = $1")
                .bind(id)
                .fetch_one(self.persist.pool())
                .await?,
        )
    }

    async fn file(&self, id: i64) -> anyhow::Result<()> {
        assert!(
            self.persist
                .web()
                .mark_wright_followup_filed(id, 9000 + i32::try_from(id)?, "https://example.test/i")
                .await?
        );
        Ok(())
    }

    async fn hand_off(&self, id: i64) -> anyhow::Result<()> {
        assert!(
            self.persist
                .web()
                .mark_wright_followup_handed_off(id, Uuid::new_v4(), "omit")
                .await?
        );
        Ok(())
    }
}

#[tokio::test]
async fn followup_same_kind_check_and_part_is_added_as_evidence_not_opened_again()
-> anyhow::Result<()> {
    let env = Env::boot().await?;
    let (problem, episode) = env.problem(CHECK, true).await?;
    let first = env.finding(problem, episode).await?;
    let second = env.finding(problem, episode).await?;
    let id = env.record("hardening", Some("launch"), first).await?;
    assert_eq!(env.record("hardening", Some("launch"), second).await?, id);
    assert_eq!(env.record("hardening", Some("launch"), second).await?, id);
    let other = env.record("hardening", Some("mailbox"), first).await?;
    assert_ne!(other, id);
    let listed = env
        .persist
        .web()
        .list_wright_followups(None, 10, None)
        .await?;
    let row = listed
        .iter()
        .find(|row| row.id == id)
        .ok_or_else(|| anyhow::anyhow!("missing"))?;
    assert_eq!(row.findings, vec![first, second]);
    assert_eq!(row.problems, vec![problem]);
    env.db.cleanup().await
}

#[tokio::test]
async fn followup_same_as_links_the_finding_to_the_named_live_followup() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let (problem, episode) = env.problem(CHECK, true).await?;
    let first = env.finding(problem, episode).await?;
    let second = env.finding(problem, episode).await?;
    let id = env.record("check_tuning", None, first).await?;
    let linked = env
        .record_same("code_defect", Some("wire"), second, Some(id))
        .await?;
    assert_eq!(linked, id);
    let rows = env
        .persist
        .web()
        .wright_followups_for_problem(problem)
        .await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].findings, vec![first, second]);
    let closed = env
        .persist
        .web()
        .mark_wright_followup_closed(id, "dropped", "not needed")
        .await?;
    assert!(closed);
    let fresh = env
        .record_same("check_tuning", None, second, Some(id))
        .await?;
    assert_ne!(fresh, id);
    env.db.cleanup().await
}

#[tokio::test]
async fn followup_root_cause_finding_joins_the_open_root_cause_of_its_check() -> anyhow::Result<()>
{
    let env = Env::boot().await?;
    let (problem, episode) = env.problem(CHECK, true).await?;
    let finding = env.finding(problem, episode).await?;
    let opened = env
        .persist
        .web()
        .open_root_cause_followup(CHECK, REMEDY, 12, 7)
        .await?
        .ok_or_else(|| anyhow::anyhow!("not opened"))?;
    assert_eq!(
        env.record("root_cause", Some("launch"), finding).await?,
        opened
    );
    let live = env
        .persist
        .web()
        .live_wright_followups(Some(CHECK), None)
        .await?;
    assert_eq!(
        live.iter().map(|row| row.id).collect::<Vec<_>>(),
        vec![opened]
    );
    assert!(
        env.persist
            .web()
            .live_wright_followups(None, None)
            .await?
            .is_empty()
    );
    env.db.cleanup().await
}

#[tokio::test]
async fn followup_recurring_remedy_opens_one_root_cause_and_is_picked_until_attempts_run_out()
-> anyhow::Result<()> {
    let env = Env::boot().await?;
    let web = env.persist.web();
    let (newest_problem, newest_episode) = env.verified_actions(3).await?;
    assert!(web.recurring_wright_remedies(7, 4).await?.is_empty());
    let found = web.recurring_wright_remedies(7, 3).await?;
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].remedy, REMEDY);
    assert_eq!(found[0].check_name, CHECK);
    assert_eq!(found[0].actions, 3);
    assert_eq!(found[0].newest_problem_id, newest_problem);
    assert_eq!(found[0].newest_episode_id, newest_episode);
    let id = web
        .open_root_cause_followup(CHECK, REMEDY, 3, 7)
        .await?
        .ok_or_else(|| anyhow::anyhow!("not opened"))?;
    assert_eq!(
        web.open_root_cause_followup(CHECK, REMEDY, 3, 7).await?,
        None
    );
    assert!(web.recurring_wright_remedies(7, 3).await?.is_empty());
    assert!(web.wright_followups_to_file(10).await?.is_empty());
    let pick = web
        .pick_wright_root_cause(2)
        .await?
        .ok_or_else(|| anyhow::anyhow!("nothing picked"))?;
    assert_eq!(
        (pick.followup_id, pick.problem_id, pick.episode_id),
        (id, newest_problem, newest_episode)
    );
    sqlx::query(
        "INSERT INTO wright.investigations (mode, problem_id, session_uuid, state, host,
                                            deadline_at, ended_at, end_reason, followup_id)
         SELECT 'investigate', $1, gen_random_uuid(), 'failed', 'test', now(), now(), 'x', $2
           FROM generate_series(1, 2)",
    )
    .bind(newest_problem)
    .bind(id)
    .execute(env.persist.pool())
    .await?;
    assert!(web.pick_wright_root_cause(2).await?.is_none());
    assert!(web.pick_wright_root_cause(3).await?.is_some());
    env.db.cleanup().await
}

#[tokio::test]
async fn followup_recurrence_needs_an_open_problem_of_the_check() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    env.verified_actions(3).await?;
    sqlx::query(
        "UPDATE wright.problems SET state = 'healthy', healthy_at = now() WHERE check_name = $1",
    )
    .bind(CHECK)
    .execute(env.persist.pool())
    .await?;
    assert!(
        env.persist
            .web()
            .recurring_wright_remedies(7, 3)
            .await?
            .is_empty()
    );
    env.db.cleanup().await
}

#[tokio::test]
async fn followup_filing_takes_only_open_followups_with_a_finding_and_counts_the_day()
-> anyhow::Result<()> {
    let env = Env::boot().await?;
    let web = env.persist.web();
    let (problem, episode) = env.problem(CHECK, true).await?;
    let finding = env.finding(problem, episode).await?;
    let id = env.record("hardening", Some("launch"), finding).await?;
    let ready = web.wright_followups_to_file(10).await?;
    assert_eq!(ready.iter().map(|row| row.id).collect::<Vec<_>>(), vec![id]);
    assert_eq!(web.wright_followups_filed_today().await?, 0);
    env.file(id).await?;
    assert!(!web.mark_wright_followup_filed(id, 1, "u").await?);
    assert_eq!(web.wright_followups_filed_today().await?, 1);
    assert!(web.wright_followups_to_file(10).await?.is_empty());
    env.db.cleanup().await
}

#[tokio::test]
async fn followup_handoff_skips_a_code_defect_the_repair_path_owns() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let web = env.persist.web();
    let (problem, episode) = env.problem(CHECK, true).await?;
    let repaired = env.finding(problem, episode).await?;
    let plain = env.finding(problem, episode).await?;
    let defect = env.record("code_defect", Some("wire"), repaired).await?;
    env.file(defect).await?;
    sqlx::query(
        "INSERT INTO wright.repairs (finding_id, problem_id, owning_part, state, worktree, branch)
         VALUES ($1, $2, 'wire', 'editing', '/w', 'b')",
    )
    .bind(repaired)
    .bind(problem)
    .execute(env.persist.pool())
    .await?;
    assert!(web.next_wright_handoff().await?.is_none());
    let hardening = env.record("hardening", Some("wire"), plain).await?;
    env.file(hardening).await?;
    let next = web
        .next_wright_handoff()
        .await?
        .ok_or_else(|| anyhow::anyhow!("no handoff"))?;
    assert_eq!(next.id, hardening);
    assert_eq!(next.findings, vec![plain]);
    assert_eq!(web.wright_handoffs_today().await?, 0);
    env.hand_off(hardening).await?;
    assert_eq!(web.wright_handoffs_today().await?, 1);
    let live = web.live_wright_handoffs().await?;
    assert_eq!(
        live.iter().map(|row| row.id).collect::<Vec<_>>(),
        vec![hardening]
    );
    assert!(web.next_wright_handoff().await?.is_none());
    env.db.cleanup().await
}

#[tokio::test]
async fn followup_returned_twice_is_stuck_and_still_blocks_a_duplicate() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let web = env.persist.web();
    let (problem, episode) = env.problem(CHECK, true).await?;
    let finding = env.finding(problem, episode).await?;
    let id = env.record("hardening", None, finding).await?;
    env.file(id).await?;
    env.hand_off(id).await?;
    assert_eq!(
        web.return_wright_followup(id, 2).await?.as_deref(),
        Some("filed")
    );
    assert_eq!(env.state_of(id).await?, ("filed".to_owned(), 1));
    env.hand_off(id).await?;
    assert_eq!(
        web.return_wright_followup(id, 2).await?.as_deref(),
        Some("stuck")
    );
    assert_eq!(env.state_of(id).await?, ("stuck".to_owned(), 2));
    assert_eq!(web.return_wright_followup(id, 2).await?, None);
    assert!(web.next_wright_handoff().await?.is_none());
    assert_eq!(env.record("hardening", None, finding).await?, id);
    assert!(
        web.mark_wright_followup_closed(id, "fixed", "merged")
            .await?
    );
    assert!(
        !web.mark_wright_followup_closed(id, "fixed", "again")
            .await?
    );
    env.db.cleanup().await
}

#[tokio::test]
async fn followup_limits_default_patch_and_refuse_a_bad_fixer() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let web = env.persist.web();
    let before = web.wright_followup_limits().await?;
    assert_eq!(
        (
            before.repo.as_str(),
            before.daily_filed,
            before.daily_handoffs,
            before.max_attempts
        ),
        ("temet-ai/mike", 3, 2, 2)
    );
    assert!(before.fixers.0.is_empty());
    let fixer = Fixer {
        machine: "infra".to_owned(),
        host: Some("infra".to_owned()),
        folder: None,
    };
    let after = web
        .patch_wright_followup_limits(&FollowupLimitsPatch {
            daily_filed: Some(5),
            fixers: Some(vec![fixer.clone()]),
            ..FollowupLimitsPatch::default()
        })
        .await?;
    assert_eq!((after.daily_filed, after.daily_handoffs), (5, 2));
    assert_eq!(after.fixers.0, vec![fixer]);
    let blank = Fixer {
        machine: " ".to_owned(),
        host: None,
        folder: None,
    };
    let refused = web
        .patch_wright_followup_limits(&FollowupLimitsPatch {
            fixers: Some(vec![blank]),
            ..FollowupLimitsPatch::default()
        })
        .await;
    assert!(refused.is_err());
    let out_of_range = web
        .patch_wright_followup_limits(&FollowupLimitsPatch {
            max_attempts: Some(9),
            ..FollowupLimitsPatch::default()
        })
        .await;
    assert!(out_of_range.is_err());
    env.db.cleanup().await
}
