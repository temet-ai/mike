use crate::store::persistence::web_repo::WebPersist as _;

use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db};

const CHECK: &str = "triage_fixture";

struct Env {
    db: TestDbHandle,
    persist: PersistHandle,
}

impl Env {
    async fn boot() -> anyhow::Result<Self> {
        let db = acquire_ephemeral_db()
            .await?
            .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
        let persist = PersistHandle::connect(&db.database_url).await?;
        sqlx::query(
            "INSERT INTO wright.checks (name, source, cadence, expects, probe, query)
             VALUES ($1, 'capture_db', 'regular', 'nothing is slow', 'sql', 'SELECT 1')",
        )
        .bind(CHECK)
        .execute(persist.pool())
        .await?;
        Ok(Self { db, persist })
    }

    async fn run(&self, check: &str) -> anyhow::Result<i64> {
        Ok(sqlx::query_scalar(
            "INSERT INTO wright.check_runs (check_name, source, cadence, started_at, outcome)
             VALUES ($1, 'capture_db', 'regular', now(), 'fail') RETURNING id",
        )
        .bind(check)
        .fetch_one(self.persist.pool())
        .await?)
    }

    async fn problem(
        &self,
        check: &str,
        resource: &str,
        run: i64,
        age_hours: i32,
    ) -> anyhow::Result<(i64, i64)> {
        let problem: i64 = sqlx::query_scalar(
            "INSERT INTO wright.problems (check_name, source, scope, resource, state, opened_at)
             VALUES ($1, 'capture_db', 'query', $2, 'open', now() - make_interval(hours => $3))
             RETURNING id",
        )
        .bind(check)
        .bind(resource)
        .bind(age_hours)
        .fetch_one(self.persist.pool())
        .await?;
        let episode: i64 = sqlx::query_scalar(
            "INSERT INTO wright.episodes (problem_id, opened_by_run, last_seen_run, evidence)
             VALUES ($1, $2, $2, '{}'::jsonb) RETURNING id",
        )
        .bind(problem)
        .bind(run)
        .fetch_one(self.persist.pool())
        .await?;
        Ok((problem, episode))
    }

    async fn cause_of(&self, problem: i64) -> anyhow::Result<Option<i64>> {
        Ok(
            sqlx::query_scalar("SELECT cause_id FROM wright.problems WHERE id = $1")
                .bind(problem)
                .fetch_one(self.persist.pool())
                .await?,
        )
    }

    async fn finding(&self, problem: i64, episode: i64) -> anyhow::Result<()> {
        sqlx::query(
            "WITH run AS (
                 INSERT INTO wright.investigations (mode, problem_id, episode_id, session_uuid, state,
                                                    host, deadline_at, ended_at)
                 VALUES ('investigate', $1, $2, gen_random_uuid(), 'finished', 'test', now(), now())
                 RETURNING id
             )
             INSERT INTO wright.findings (investigation_id, problem_id, episode_id, symptom,
                                          suspected_cause, cause_established, kind, remedy, evidence)
             SELECT run.id, $1, $2, 'slow', 'one query', false, 'unknown', 'index it',
                    '[{\"kind\": \"problem\", \"ref\": \"fixture\"}]'::jsonb
               FROM run",
        )
        .bind(problem)
        .bind(episode)
        .execute(self.persist.pool())
        .await?;
        Ok(())
    }

    async fn done(self) -> anyhow::Result<()> {
        self.db.cleanup().await
    }
}

#[tokio::test]
async fn problems_differing_only_in_numbers_share_a_cause_and_a_new_shape_gets_its_own()
-> anyhow::Result<()> {
    let env = Env::boot().await?;
    let run = env.run(CHECK).await?;
    let (a, _) = env
        .problem(CHECK, "query -621548: WITH own_turn AS (SELECT 1)", run, 1)
        .await?;
    let (b, _) = env
        .problem(CHECK, "query 812345: WITH own_turn AS (SELECT 1)", run, 1)
        .await?;
    let (c, _) = env
        .problem(CHECK, "query 4030887: SELECT u.file_path", run, 1)
        .await?;
    let assigned = env.persist.web().assign_wright_causes().await?;
    assert_eq!((assigned.assigned, assigned.opened), (3, 2));
    assert_eq!(env.cause_of(a).await?, env.cause_of(b).await?);
    assert_ne!(env.cause_of(a).await?, env.cause_of(c).await?);
    env.done().await
}

#[tokio::test]
async fn a_run_grouped_check_puts_the_problems_one_run_opened_in_one_cause() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    sqlx::query(
        "UPDATE wright.checks SET params = params || '{\"group_by\": \"run\"}' WHERE name = $1",
    )
    .bind(CHECK)
    .execute(env.persist.pool())
    .await?;
    let first = env.run(CHECK).await?;
    let second = env.run(CHECK).await?;
    let (a, _) = env.problem(CHECK, "leaf_judge", first, 1).await?;
    let (b, _) = env.problem(CHECK, "topic_merge", first, 1).await?;
    let (c, _) = env.problem(CHECK, "directive", second, 1).await?;
    env.persist.web().assign_wright_causes().await?;
    assert_eq!(env.cause_of(a).await?, env.cause_of(b).await?);
    assert_ne!(env.cause_of(a).await?, env.cause_of(c).await?);
    env.done().await
}

#[tokio::test]
async fn the_pick_takes_a_shared_cause_before_an_older_lone_problem_and_skips_a_found_cause()
-> anyhow::Result<()> {
    let env = Env::boot().await?;
    let run = env.run(CHECK).await?;
    let (lone, lone_episode) = env.problem(CHECK, "query 1: SELECT lone", run, 30).await?;
    let mut shared = Vec::new();
    for n in 0..4 {
        shared.push(
            env.problem(
                CHECK,
                &format!("query {n}9: WITH shared AS (SELECT 1)"),
                run,
                1,
            )
            .await?,
        );
    }
    env.persist.web().assign_wright_causes().await?;
    let pick = env
        .persist
        .web()
        .pick_wright_problem(2)
        .await?
        .expect("a pick");
    assert!(shared.iter().any(|(p, _)| *p == pick.problem_id));
    env.finding(shared[3].0, shared[3].1).await?;
    let pick = env
        .persist
        .web()
        .pick_wright_problem(2)
        .await?
        .expect("a pick");
    assert_eq!((pick.problem_id, pick.episode_id), (lone, lone_episode));
    env.done().await
}

#[tokio::test]
async fn merging_moves_the_named_causes_problems_into_the_finding_problems_cause()
-> anyhow::Result<()> {
    let env = Env::boot().await?;
    let run = env.run(CHECK).await?;
    let (root, _) = env
        .problem(CHECK, "query 1: WITH a AS (SELECT 1)", run, 1)
        .await?;
    let (other, _) = env.problem(CHECK, "query 2: SELECT b", run, 1).await?;
    let (third, _) = env.problem(CHECK, "query 3: SELECT c", run, 1).await?;
    env.persist.web().assign_wright_causes().await?;
    let other_cause = env.cause_of(other).await?.expect("assigned");
    let merged = env
        .persist
        .web()
        .merge_wright_causes(root, &[other_cause])
        .await?;
    assert_eq!((merged.causes, merged.problems), (1, 1));
    assert_eq!(env.cause_of(other).await?, env.cause_of(root).await?);
    assert_ne!(env.cause_of(third).await?, env.cause_of(root).await?);
    let (again, _) = env.problem(CHECK, "query 9: SELECT b", run, 1).await?;
    env.persist.web().assign_wright_causes().await?;
    assert_eq!(env.cause_of(again).await?, env.cause_of(root).await?);
    env.done().await
}
