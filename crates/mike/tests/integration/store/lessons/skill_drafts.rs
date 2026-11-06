use mike::store::persistence::PersistHandle;
use mike::store::persistence::lessons::NewLessonWrite;
use mike::store::persistence::lessons::skill_drafts::{DraftTarget, NewSkillDraft};
use mike::store::test_support::{self, TestDb, TestDbHandle};
use serde_json::json;

const TOPIC: &str = "database-migrations";

pub(super) struct Fixture {
    pub(super) db: TestDbHandle,
    pub(super) handle: PersistHandle,
    pub(super) project_id: i64,
}

pub(super) async fn fixture() -> anyhow::Result<Option<Fixture>> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping skill_drafts test");
        return Ok(None);
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project_id = test_support::seed_project(handle.pool(), "/tmp/skill-drafts").await?;
    Ok(Some(Fixture {
        db,
        handle,
        project_id,
    }))
}

impl Fixture {
    pub(super) async fn daily_lesson(&self, text: &str) -> anyhow::Result<i64> {
        let row = self
            .handle
            .create_lesson_with_topic(NewLessonWrite {
                project_id: self.project_id,
                kind: "convention",
                text,
                topic: Some(TOPIC),
                source_turn_id: None,
                agent_id: None,
                session_db_id: None,
                request_id: None,
            })
            .await?;
        sqlx::query(
            "UPDATE memory.lessons
                SET stage_id = (SELECT id FROM memory.lesson_stages WHERE code = 'daily'),
                    placement_id = (SELECT id FROM memory.lesson_placements WHERE code = 'skill'),
                    placed_at = now()
              WHERE id = $1",
        )
        .bind(row.id)
        .execute(self.handle.pool())
        .await?;
        Ok(row.id)
    }

    async fn age_failures(&self, hours: i32) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE memory.skill_draft_failures
                SET last_failed_at = now() - make_interval(hours => $1)",
        )
        .bind(hours)
        .execute(self.handle.pool())
        .await?;
        Ok(())
    }

    pub(super) async fn finish(self) -> anyhow::Result<()> {
        self.handle.pool().close().await;
        self.db.cleanup().await
    }
}

pub(super) fn draft<'a>(
    target: &'a DraftTarget,
    ids: &'a [i64],
    empty: &'a serde_json::Value,
) -> NewSkillDraft<'a> {
    NewSkillDraft {
        target,
        lesson_ids: ids,
        model: "test-model",
        description: "Ship a migration safely.",
        skill_md: "---\nname: database-migrations\n---\nbody",
        reference_files: empty,
        trigger_queries: empty,
        generation_id: None,
        cost_usd: None,
    }
}

#[tokio::test]
async fn next_target_with_two_daily_lessons_is_offered_until_drafted() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let first = fx.daily_lesson("Run migrate-check first.").await?;
    let second = fx
        .daily_lesson("Commit migrations with their code.")
        .await?;

    let target = fx
        .handle
        .next_skill_draft_target()
        .await?
        .ok_or_else(|| anyhow::anyhow!("a topic with two daily lessons must be a target"))?;
    assert_eq!(target.folder, TOPIC);
    assert_eq!(target.lesson_watermark, second);

    let lessons = fx.handle.skill_draft_lessons(&target).await?;
    let ids: Vec<i64> = lessons.iter().map(|l| l.id).collect();
    assert_eq!(ids, vec![first, second]);
    assert!(lessons.iter().all(|l| l.kind == "convention"));

    let empty = json!([]);
    fx.handle
        .insert_skill_draft(draft(&target, &ids, &empty))
        .await?;
    assert!(
        fx.handle.next_skill_draft_target().await?.is_none(),
        "a drafted topic must not be offered again at the same watermark"
    );

    let third = fx
        .daily_lesson("Number a migration above the applied one.")
        .await?;
    let again = fx
        .handle
        .next_skill_draft_target()
        .await?
        .ok_or_else(|| anyhow::anyhow!("a new lesson must reopen the topic"))?;
    assert_eq!(again.lesson_watermark, third);
    fx.finish().await
}

#[tokio::test]
async fn failed_target_backs_off_and_stops_after_three_attempts() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    fx.daily_lesson("Run migrate-check first.").await?;
    fx.daily_lesson("Commit migrations with their code.")
        .await?;
    let target = fx
        .handle
        .next_skill_draft_target()
        .await?
        .ok_or_else(|| anyhow::anyhow!("target before any failure"))?;

    assert_eq!(
        fx.handle
            .record_skill_draft_failure(&target, "model: 502")
            .await?,
        1
    );
    assert!(
        fx.handle.next_skill_draft_target().await?.is_none(),
        "a topic that just failed must wait"
    );

    fx.age_failures(3).await?;
    assert!(
        fx.handle.next_skill_draft_target().await?.is_some(),
        "after two hours the first failure must allow a retry"
    );

    fx.handle
        .record_skill_draft_failure(&target, "rejected draft")
        .await?;
    assert_eq!(
        fx.handle
            .record_skill_draft_failure(&target, "rejected draft")
            .await?,
        3
    );
    fx.age_failures(1000).await?;
    assert!(
        fx.handle.next_skill_draft_target().await?.is_none(),
        "three failures at one watermark must stop the topic"
    );

    fx.daily_lesson("Number a migration above the applied one.")
        .await?;
    assert!(
        fx.handle.next_skill_draft_target().await?.is_some(),
        "a new lesson must reopen a stopped topic"
    );
    fx.finish().await
}

#[tokio::test]
async fn insert_skill_draft_a_fourth_time_at_one_watermark_is_err() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let a = fx.daily_lesson("Run migrate-check first.").await?;
    let b = fx
        .daily_lesson("Commit migrations with their code.")
        .await?;
    let target = fx
        .handle
        .next_skill_draft_target()
        .await?
        .ok_or_else(|| anyhow::anyhow!("target"))?;
    let empty = json!([]);
    let ids = [a, b];
    for _ in 0..3 {
        fx.handle
            .insert_skill_draft(draft(&target, &ids, &empty))
            .await?;
    }
    assert!(
        fx.handle
            .insert_skill_draft(draft(&target, &ids, &empty))
            .await
            .is_err()
    );
    let revisions: Vec<i32> = sqlx::query_scalar(
        "SELECT revision FROM memory.skill_drafts WHERE project_id = $1 ORDER BY id",
    )
    .bind(fx.project_id)
    .fetch_all(fx.handle.pool())
    .await?;
    assert_eq!(revisions, vec![1, 2, 3]);
    fx.finish().await
}

#[tokio::test]
async fn record_failure_for_unknown_project_is_err() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let target = DraftTarget {
        project_id: fx.project_id + 1_000_000,
        project_path: "/tmp/none".to_owned(),
        folder: TOPIC.to_owned(),
        topic: TOPIC.to_owned(),
        agent_id: None,
        agent_name: None,
        lesson_watermark: 1,
    };
    assert!(
        fx.handle
            .record_skill_draft_failure(&target, "x")
            .await
            .is_err()
    );
    fx.finish().await
}

#[tokio::test]
async fn reads_on_a_closed_pool_are_err() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let target = DraftTarget {
        project_id: fx.project_id,
        project_path: "/tmp/skill-drafts".to_owned(),
        folder: TOPIC.to_owned(),
        topic: TOPIC.to_owned(),
        agent_id: None,
        agent_name: None,
        lesson_watermark: 1,
    };
    fx.handle.pool().close().await;
    assert!(fx.handle.next_skill_draft_target().await.is_err());
    assert!(fx.handle.skill_draft_lessons(&target).await.is_err());
    fx.db.cleanup().await
}
