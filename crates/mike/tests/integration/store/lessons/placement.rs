use mike::store::persistence::lessons::NewLessonWrite;
use mike::store::persistence::lessons::placement::{PlacementWrite, TopicMove};
use mike::store::persistence::lessons_repo::LessonsPersist as _;
use serde_json::json;

use super::skill_drafts::{Fixture, draft, fixture};

async fn agent(fx: &Fixture, name: &str) -> anyhow::Result<i64> {
    Ok(
        sqlx::query_scalar("INSERT INTO agents.agents (name) VALUES ($1) RETURNING id")
            .bind(name)
            .fetch_one(fx.handle.pool())
            .await?,
    )
}

async fn lesson(
    fx: &Fixture,
    text: &str,
    topic: &str,
    placement: Option<&str>,
    agent_id: Option<i64>,
) -> anyhow::Result<i64> {
    let row = fx
        .handle
        .lessons()
        .create_lesson_with_topic(NewLessonWrite {
            project_id: fx.project_id,
            kind: "convention",
            text,
            topic: Some(topic),
            source_turn_id: None,
            agent_id,
            session_db_id: None,
            request_id: None,
        })
        .await?;
    sqlx::query(
        "UPDATE memory.lessons
            SET stage_id = (SELECT id FROM memory.lesson_stages WHERE code = 'daily')
          WHERE id = $1",
    )
    .bind(row.id)
    .execute(fx.handle.pool())
    .await?;
    if let Some(home) = placement {
        let write = PlacementWrite {
            home,
            covered_by: None,
            reason: "test",
        };
        assert!(
            fx.handle
                .lessons()
                .set_lesson_placement(row.id, write)
                .await?
        );
    }
    Ok(row.id)
}

#[tokio::test]
async fn unplaced_lessons_come_oldest_first_and_a_placement_is_written_once() -> anyhow::Result<()>
{
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let first = lesson(&fx, "Run migrate-check first.", "migrations", None, None).await?;
    let second = lesson(
        &fx,
        "Number migrations above main.",
        "migrations",
        None,
        None,
    )
    .await?;
    let placed = lesson(&fx, "Already placed.", "migrations", Some("memory"), None).await?;

    let ids: Vec<i64> = fx
        .handle
        .lessons()
        .unplaced_lessons(10)
        .await?
        .into_iter()
        .map(|l| l.id)
        .collect();
    assert_eq!(ids, vec![first, second]);
    assert!(!ids.contains(&placed));

    let write = PlacementWrite {
        home: "rule",
        covered_by: Some("rules/migrations.md"),
        reason: "tied to files",
    };
    assert!(
        fx.handle
            .lessons()
            .set_lesson_placement(first, write)
            .await?
    );
    let again = PlacementWrite {
        home: "skill",
        covered_by: None,
        reason: "changed my mind",
    };
    assert!(
        !fx.handle
            .lessons()
            .set_lesson_placement(first, again)
            .await?
    );
    let stored: (String, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT lp.code, l.placement_covered_by, l.placement_reason
           FROM memory.lessons l JOIN memory.lesson_placements lp ON lp.id = l.placement_id
          WHERE l.id = $1",
    )
    .bind(first)
    .fetch_one(fx.handle.pool())
    .await?;
    assert_eq!(
        stored,
        (
            "rule".to_owned(),
            Some("rules/migrations.md".to_owned()),
            Some("tied to files".to_owned())
        )
    );
    fx.finish().await
}

#[tokio::test]
async fn an_unknown_home_places_nothing() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let id = lesson(&fx, "Run migrate-check first.", "migrations", None, None).await?;
    let write = PlacementWrite {
        home: "attic",
        covered_by: None,
        reason: "x",
    };
    assert!(!fx.handle.lessons().set_lesson_placement(id, write).await?);
    assert_eq!(fx.handle.lessons().unplaced_lessons(10).await?.len(), 1);
    fx.finish().await
}

#[tokio::test]
async fn a_failing_lesson_is_withheld_for_a_wait_and_after_three_failures_for_good()
-> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let id = lesson(&fx, "Run migrate-check first.", "migrations", None, None).await?;
    assert_eq!(
        fx.handle
            .lessons()
            .record_placement_failure(id, "boom")
            .await?,
        1
    );
    assert!(fx.handle.lessons().unplaced_lessons(10).await?.is_empty());

    sqlx::query(
        "UPDATE memory.lesson_placement_failures SET last_failed_at = now() - interval '3 hours'",
    )
    .execute(fx.handle.pool())
    .await?;
    assert_eq!(fx.handle.lessons().unplaced_lessons(10).await?.len(), 1);

    assert_eq!(
        fx.handle
            .lessons()
            .record_placement_failure(id, "boom")
            .await?,
        2
    );
    assert_eq!(
        fx.handle
            .lessons()
            .record_placement_failure(id, "boom")
            .await?,
        3
    );
    sqlx::query(
        "UPDATE memory.lesson_placement_failures SET last_failed_at = now() - interval '100 days'",
    )
    .execute(fx.handle.pool())
    .await?;
    assert!(fx.handle.lessons().unplaced_lessons(10).await?.is_empty());
    fx.finish().await
}

#[tokio::test]
async fn only_skill_placed_and_unplaced_lessons_make_a_draft_target() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    lesson(
        &fx,
        "Number migrations above main.",
        "migrations",
        Some("rule"),
        None,
    )
    .await?;
    lesson(
        &fx,
        "Run migrate-check first.",
        "migrations",
        Some("memory"),
        None,
    )
    .await?;
    let unplaced = lesson(&fx, "The DB is on omit.", "migrations", None, None).await?;
    assert!(
        fx.handle
            .lessons()
            .next_skill_draft_target()
            .await?
            .is_none(),
        "one unplaced lesson among others placed elsewhere is not enough"
    );

    let a = lesson(
        &fx,
        "Check status after deploy.",
        "migrations",
        Some("skill"),
        None,
    )
    .await?;
    let target = fx
        .handle
        .lessons()
        .next_skill_draft_target()
        .await?
        .ok_or_else(|| anyhow::anyhow!("an unplaced and a skill lesson must be a target"))?;
    assert_eq!(target.lesson_watermark, a);
    let b = lesson(
        &fx,
        "Restart with start-proxy.sh.",
        "migrations",
        Some("skill"),
        None,
    )
    .await?;
    let target = fx
        .handle
        .lessons()
        .next_skill_draft_target()
        .await?
        .ok_or_else(|| anyhow::anyhow!("three eligible lessons must be a target"))?;
    assert_eq!(target.lesson_watermark, b);
    let ids: Vec<i64> = fx
        .handle
        .lessons()
        .skill_draft_lessons(&target)
        .await?
        .into_iter()
        .map(|l| l.id)
        .collect();
    assert_eq!(ids, vec![unplaced, a, b]);
    fx.finish().await
}

#[tokio::test]
async fn placing_an_unplaced_lesson_elsewhere_takes_it_out_of_the_draft() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let first = lesson(&fx, "Check status after deploy.", "deploy", None, None).await?;
    let second = lesson(&fx, "Restart with start-proxy.sh.", "deploy", None, None).await?;
    let target = fx
        .handle
        .lessons()
        .next_skill_draft_target()
        .await?
        .ok_or_else(|| anyhow::anyhow!("two unplaced lessons must be a target"))?;
    assert_eq!(target.lesson_watermark, second);

    let write = PlacementWrite {
        home: "memory",
        covered_by: None,
        reason: "a fact",
    };
    assert!(
        fx.handle
            .lessons()
            .set_lesson_placement(second, write)
            .await?
    );
    assert!(
        fx.handle
            .lessons()
            .next_skill_draft_target()
            .await?
            .is_none()
    );
    let ids: Vec<i64> = fx
        .handle
        .lessons()
        .skill_draft_lessons(&target)
        .await?
        .into_iter()
        .map(|l| l.id)
        .collect();
    assert_eq!(ids, vec![first]);
    fx.finish().await
}

#[tokio::test]
async fn the_agent_is_named_only_when_one_agent_owns_every_lesson_of_the_topic()
-> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let rust = agent(&fx, "rust-developer").await?;
    let db = agent(&fx, "db-dev").await?;
    lesson(
        &fx,
        "Check status after deploy.",
        "deploy",
        Some("skill"),
        Some(rust),
    )
    .await?;
    lesson(
        &fx,
        "Restart with start-proxy.sh.",
        "deploy",
        Some("skill"),
        Some(rust),
    )
    .await?;
    let single = fx
        .handle
        .lessons()
        .next_skill_draft_target()
        .await?
        .ok_or_else(|| anyhow::anyhow!("target"))?;
    assert_eq!(single.folder, "deploy-rust-developer");
    assert_eq!(single.agent_id, Some(rust));

    lesson(
        &fx,
        "Deploy from main only.",
        "deploy",
        Some("skill"),
        Some(db),
    )
    .await?;
    let shared = fx
        .handle
        .lessons()
        .next_skill_draft_target()
        .await?
        .ok_or_else(|| anyhow::anyhow!("target"))?;
    assert_eq!((shared.folder.as_str(), shared.agent_id), ("deploy", None));
    assert_eq!(
        fx.handle
            .lessons()
            .skill_draft_lessons(&shared)
            .await?
            .len(),
        3
    );
    fx.finish().await
}

#[tokio::test]
async fn agent_scoped_lessons_not_placed_as_skills_make_no_agent_skill() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let rust = agent(&fx, "rust-developer").await?;
    lesson(
        &fx,
        "Check status after deploy.",
        "deploy",
        Some("memory"),
        Some(rust),
    )
    .await?;
    lesson(
        &fx,
        "Restart with start-proxy.sh.",
        "deploy",
        Some("agent"),
        Some(rust),
    )
    .await?;
    assert!(
        fx.handle
            .lessons()
            .next_skill_draft_target()
            .await?
            .is_none()
    );
    fx.finish().await
}

#[tokio::test]
async fn a_draft_remembers_its_topic_and_a_shipped_topic_is_marked() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    lesson(
        &fx,
        "Check status after deploy.",
        "deploying-the-proxy",
        Some("skill"),
        None,
    )
    .await?;
    lesson(
        &fx,
        "Restart with start-proxy.sh.",
        "deploying-the-proxy",
        Some("skill"),
        None,
    )
    .await?;
    lesson(
        &fx,
        "Wait for the fleet.",
        "proxy-deploy-steps",
        Some("skill"),
        None,
    )
    .await?;
    lesson(
        &fx,
        "Use --during-a-fleet.",
        "proxy-deploy-steps",
        Some("skill"),
        None,
    )
    .await?;
    lesson(&fx, "Not a skill.", "other", Some("memory"), None).await?;

    let before = fx.handle.lessons().skill_purposes().await?;
    let seen: Vec<(String, i64, bool)> = before
        .iter()
        .map(|p| (p.topic.clone(), p.lessons, p.shipped))
        .collect();
    assert_eq!(
        seen,
        vec![
            ("deploying-the-proxy".to_owned(), 2, false),
            ("proxy-deploy-steps".to_owned(), 2, false)
        ]
    );

    let target = fx
        .handle
        .lessons()
        .next_skill_draft_target()
        .await?
        .ok_or_else(|| anyhow::anyhow!("target"))?;
    let empty = json!([]);
    let id = fx
        .handle
        .lessons()
        .insert_skill_draft(draft(&target, &[1, 2], &empty))
        .await?;
    let topic: Option<String> =
        sqlx::query_scalar("SELECT topic FROM memory.skill_drafts WHERE id = $1")
            .bind(id)
            .fetch_one(fx.handle.pool())
            .await?;
    assert_eq!(topic.as_deref(), Some(target.topic.as_str()));

    sqlx::query("UPDATE memory.skill_drafts SET status = 'accepted' WHERE id = $1")
        .bind(id)
        .execute(fx.handle.pool())
        .await?;
    let after = fx.handle.lessons().skill_purposes().await?;
    let shipped: Vec<&str> = after
        .iter()
        .filter(|p| p.shipped)
        .map(|p| p.topic.as_str())
        .collect();
    assert_eq!(shipped, vec![target.topic.as_str()]);
    fx.finish().await
}

#[tokio::test]
async fn retopic_moves_only_skill_placed_lessons_of_the_named_topic() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let moved = lesson(
        &fx,
        "Wait for the fleet.",
        "proxy-deploy-steps",
        Some("skill"),
        None,
    )
    .await?;
    let stays = lesson(&fx, "A fact.", "proxy-deploy-steps", Some("memory"), None).await?;
    assert_eq!(
        fx.handle
            .lessons()
            .retopic_skill_lessons(fx.project_id, &[])
            .await?,
        0
    );
    let changed = fx
        .handle
        .lessons()
        .retopic_skill_lessons(
            fx.project_id,
            &[TopicMove {
                from: "proxy-deploy-steps".to_owned(),
                to: "deploying-the-proxy".to_owned(),
            }],
        )
        .await?;
    assert_eq!(changed, 1);
    let topic_of = |id: i64| {
        let pool = fx.handle.pool().clone();
        async move {
            sqlx::query_scalar::<_, Option<String>>(
                "SELECT topic FROM memory.lessons WHERE id = $1",
            )
            .bind(id)
            .fetch_one(&pool)
            .await
        }
    };
    assert_eq!(
        topic_of(moved).await?.as_deref(),
        Some("deploying-the-proxy")
    );
    assert_eq!(
        topic_of(stays).await?.as_deref(),
        Some("proxy-deploy-steps")
    );
    fx.finish().await
}
