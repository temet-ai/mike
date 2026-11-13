use mike::store::persistence::lessons_repo::LessonsPersist as _;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

use mike::lesson_shape::{Assessment, Shape, assess};
use mike::store::persistence::PersistHandle;
use mike::store::persistence::lessons::NewLessonWrite;
use mike::store::persistence::lessons::shape_apply::{RewrittenRule, ShapeRewrite};
use mike::store::persistence::test_support;

use crate::test_db::TestDb;

const LONG_TEXT: &str = "Verify every claim against the running system before reporting it. ";

struct Fixture {
    handle: PersistHandle,
    pool: PgPool,
    project_id: i64,
    project_path: String,
}

async fn fixture() -> anyhow::Result<Option<Fixture>> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(None);
    };
    let db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;
    let project_path = format!("w-shape-{}", uuid::Uuid::new_v4());
    let project_id = test_support::seed_project(&pool, &project_path).await?;
    Ok(Some(Fixture {
        handle,
        pool,
        project_id,
        project_path,
    }))
}

fn write<'a>(f: &Fixture, text: &'a str, topic: Option<&'a str>) -> NewLessonWrite<'a> {
    NewLessonWrite {
        project_id: f.project_id,
        kind: "convention",
        text,
        topic,
        source_turn_id: None,
        agent_id: None,
        session_db_id: None,
        request_id: None,
    }
}

async fn shaped_with(
    f: &Fixture,
    text: &str,
    topic: Option<&str>,
    hold: bool,
) -> anyhow::Result<(i64, Assessment)> {
    let assessment = assess(text, topic, 500);
    let row = f
        .handle
        .lessons()
        .create_lesson_with_shape(write(f, text, topic), &assessment, hold)
        .await?;
    Ok((row.id, assessment))
}

async fn shaped(f: &Fixture, text: &str, topic: Option<&str>) -> anyhow::Result<(i64, Assessment)> {
    shaped_with(f, text, topic, true).await
}

async fn stage_and_reason(f: &Fixture, id: i64) -> anyhow::Result<(String, Option<String>)> {
    Ok(sqlx::query_as(
        "SELECT ls.code, ar.code
           FROM memory.lessons l
           JOIN memory.lesson_stages ls ON ls.id = l.stage_id
           LEFT JOIN memory.lesson_archived_reasons ar ON ar.id = l.archived_reason_id
          WHERE l.id = $1",
    )
    .bind(id)
    .fetch_one(&f.pool)
    .await?)
}

fn rule(text: &str) -> RewrittenRule {
    RewrittenRule {
        text: text.to_owned(),
        embedding: None,
    }
}

#[tokio::test]
async fn a_rule_shaped_lesson_is_recorded_as_needing_nothing_and_stays_claimable()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let (id, assessment) = shaped(&f, "Run scripts/gate lint before committing.", None).await?;
    assert!(assessment.is_rule());
    let finding = f.handle.lessons().shape_finding(id).await?;
    let Some(finding) = finding else {
        anyhow::bail!("a finding row must exist for a checked lesson");
    };
    assert_eq!(finding.shape, "rule");
    assert_eq!(finding.resolution.as_deref(), Some("none_needed"));
    let claimed = f
        .handle
        .lessons()
        .claim_raw_lessons_for_consolidation(f.project_id, 10)
        .await?;
    assert!(claimed.iter().any(|l| l.id == id));
    Ok(())
}

#[tokio::test]
async fn a_flagged_lesson_is_held_back_from_consolidation_idle_discovery_and_injection()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let long = LONG_TEXT.repeat(12);
    let (flagged, assessment) = shaped(&f, &long, None).await?;
    assert_eq!(assessment.shape, Shape::OverLength);
    let (fine, _) = shaped(&f, "Run scripts/gate lint before committing.", None).await?;

    let claimed = f
        .handle
        .lessons()
        .claim_raw_lessons_for_consolidation(f.project_id, 10)
        .await?;
    let ids: Vec<i64> = claimed.iter().map(|l| l.id).collect();
    assert!(ids.contains(&fine));
    assert!(!ids.contains(&flagged), "claimed {ids:?}");

    let injectable = f
        .handle
        .lessons()
        .list_injectable_lessons(&f.project_path)
        .await?;
    let injectable_ids: Vec<i64> = injectable.iter().map(|l| l.id).collect();
    assert!(injectable_ids.contains(&fine));
    assert!(!injectable_ids.contains(&flagged), "{injectable_ids:?}");
    assert!(
        f.handle
            .lessons()
            .lesson_has_open_shape_finding(flagged)
            .await?
    );
    assert!(
        !f.handle
            .lessons()
            .lesson_has_open_shape_finding(fine)
            .await?
    );
    Ok(())
}

#[tokio::test]
async fn without_the_proposer_a_flagged_lesson_is_recorded_as_kept_and_not_held()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let (id, assessment) = shaped_with(&f, &LONG_TEXT.repeat(12), None, false).await?;
    assert_eq!(assessment.shape, Shape::OverLength);
    let finding = f.handle.lessons().shape_finding(id).await?;
    assert_eq!(finding.and_then(|r| r.resolution), Some("kept".to_owned()));
    assert!(!f.handle.lessons().lesson_has_open_shape_finding(id).await?);
    let claimed = f
        .handle
        .lessons()
        .claim_raw_lessons_for_consolidation(f.project_id, 10)
        .await?;
    assert!(claimed.iter().any(|l| l.id == id));
    let injectable = f
        .handle
        .lessons()
        .list_injectable_lessons(&f.project_path)
        .await?;
    assert!(injectable.iter().any(|l| l.id == id));
    Ok(())
}

#[tokio::test]
async fn a_hold_left_open_for_two_weeks_is_released() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let (old, _) = shaped(&f, &LONG_TEXT.repeat(12), None).await?;
    let (recent, _) = shaped(&f, &LONG_TEXT.repeat(13), None).await?;
    sqlx::query(
        "UPDATE memory.lesson_shape_findings SET found_at = now() - interval '15 days' WHERE lesson_id = $1",
    )
    .bind(old)
    .execute(&f.pool)
    .await?;
    assert!(f.handle.lessons().release_stale_shape_findings(14).await? >= 1);
    assert!(
        !f.handle
            .lessons()
            .lesson_has_open_shape_finding(old)
            .await?
    );
    assert!(
        f.handle
            .lessons()
            .lesson_has_open_shape_finding(recent)
            .await?
    );
    let released = f.handle.lessons().shape_finding(old).await?;
    assert_eq!(released.and_then(|r| r.resolution), Some("kept".to_owned()));
    Ok(())
}

#[tokio::test]
async fn the_scan_records_a_finding_for_each_unchecked_lesson_once() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let inserted = f
        .handle
        .lessons()
        .create_lesson_with_topic(write(&f, "Satisfied. The work was committed.", None))
        .await?;
    let unchecked = f
        .handle
        .lessons()
        .unchecked_lessons_for_shape(100_000)
        .await?;
    let Some(found) = unchecked.iter().find(|l| l.id == inserted.id) else {
        anyhow::bail!("the unchecked lesson must be listed");
    };
    let findings = vec![(found.id, assess(&found.text, found.topic.as_deref(), 500))];
    assert_eq!(
        f.handle.lessons().record_shape_findings(&findings).await?,
        1
    );
    assert_eq!(
        f.handle.lessons().record_shape_findings(&findings).await?,
        0
    );
    let again = f
        .handle
        .lessons()
        .unchecked_lessons_for_shape(100_000)
        .await?;
    assert!(!again.iter().any(|l| l.id == inserted.id));
    let finding = f.handle.lessons().shape_finding(inserted.id).await?;
    assert_eq!(finding.map(|r| r.shape), Some("status".to_owned()));
    Ok(())
}

#[tokio::test]
async fn rewriting_archives_the_original_with_its_text_and_links_each_rule_to_it()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let long = LONG_TEXT.repeat(12);
    let (original, _) = shaped(&f, &long, Some("verification")).await?;
    let rules = [
        rule("Verify a claim against the running system before reporting it."),
        rule("Report what was checked, not what was assumed."),
    ];
    let ids = f
        .handle
        .lessons()
        .rewrite_lesson_into_rules(&ShapeRewrite {
            lesson_id: original,
            rules: &rules,
            model: "test-model",
            original_chars: long.chars().count(),
            rationale: "two independent rules",
        })
        .await?
        .unwrap_or_default();
    assert_eq!(ids.len(), 2);

    let (stage, reason) = stage_and_reason(&f, original).await?;
    assert_eq!(
        (stage.as_str(), reason.as_deref()),
        ("archived", Some("superseded"))
    );
    let kept: String = sqlx::query_scalar("SELECT text FROM memory.lessons WHERE id = $1")
        .bind(original)
        .fetch_one(&f.pool)
        .await?;
    assert_eq!(kept, long, "the original text must stay readable");

    let relations: Vec<(i64, String, String, String)> = sqlx::query_as(
        "SELECT r.from_lesson_id, k.code, m.code, s.code
           FROM memory.lesson_relations r
           JOIN memory.lesson_relation_kinds k ON k.id = r.kind_id
           JOIN memory.lesson_relation_methods m ON m.id = r.method_id
           JOIN memory.lesson_relation_statuses s ON s.id = r.status_id
          WHERE r.to_lesson_id = $1
          ORDER BY r.from_lesson_id",
    )
    .bind(original)
    .fetch_all(&f.pool)
    .await?;
    assert_eq!(relations.len(), 2);
    for (from, kind, method, status) in &relations {
        assert!(ids.contains(from));
        assert_eq!(
            (kind.as_str(), method.as_str(), status.as_str()),
            ("supersedes", "rewritten", "confirmed")
        );
    }

    let finding = f.handle.lessons().shape_finding(original).await?;
    let Some(finding) = finding else {
        anyhow::bail!("the original keeps its finding");
    };
    assert_eq!(finding.resolution.as_deref(), Some("rewritten"));
    assert_eq!(finding.replacement_lesson_id, ids.first().copied());
    for id in &ids {
        let replacement = f.handle.lessons().shape_finding(*id).await?;
        assert_eq!(replacement.map(|r| r.shape), Some("rule".to_owned()));
    }
    let (topic, stage): (Option<String>, String) = sqlx::query_as(
        "SELECT l.topic, ls.code FROM memory.lessons l
           JOIN memory.lesson_stages ls ON ls.id = l.stage_id WHERE l.id = $1",
    )
    .bind(ids[0])
    .fetch_one(&f.pool)
    .await?;
    assert_eq!(topic.as_deref(), Some("verification"));
    assert_eq!(stage, "raw");

    let claimed = f
        .handle
        .lessons()
        .claim_raw_lessons_for_consolidation(f.project_id, 10)
        .await?;
    let claimed_ids: Vec<i64> = claimed.iter().map(|l| l.id).collect();
    assert!(ids.iter().all(|id| claimed_ids.contains(id)));
    Ok(())
}

#[tokio::test]
async fn rewriting_an_already_archived_lesson_writes_nothing() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let (original, _) = shaped(&f, &LONG_TEXT.repeat(12), None).await?;
    assert!(f.handle.lessons().delete_lesson(original).await?);
    let outcome = f
        .handle
        .lessons()
        .rewrite_lesson_into_rules(&ShapeRewrite {
            lesson_id: original,
            rules: &[rule("Verify a claim before reporting it.")],
            model: "test-model",
            original_chars: 10,
            rationale: "x",
        })
        .await?;
    assert_eq!(outcome, None);
    Ok(())
}

#[tokio::test]
async fn retiring_a_status_note_archives_it_as_not_a_lesson() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let (id, assessment) = shaped(
        &f,
        "Left unmet when the session ended: nothing ran.",
        Some("verifier:unmet"),
    )
    .await?;
    assert_eq!(assessment.shape, Shape::Status);
    assert!(f.handle.lessons().retire_lesson_as_not_a_lesson(id).await?);
    assert!(!f.handle.lessons().retire_lesson_as_not_a_lesson(id).await?);
    let (stage, reason) = stage_and_reason(&f, id).await?;
    assert_eq!(
        (stage.as_str(), reason.as_deref()),
        ("archived", Some("not_a_lesson"))
    );
    let finding = f.handle.lessons().shape_finding(id).await?;
    assert_eq!(
        finding.and_then(|r| r.resolution),
        Some("not_a_lesson".to_owned())
    );
    Ok(())
}

#[tokio::test]
async fn three_failures_keep_the_lesson_and_release_it_for_consolidation() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let (id, _) = shaped(&f, &LONG_TEXT.repeat(12), None).await?;
    let work = f.handle.lessons().claim_shape_work(100_000).await?;
    assert!(work.iter().any(|w| w.lesson_id == id));
    assert!(!f.handle.lessons().record_shape_failure(id, "first").await?);
    let after_first = f.handle.lessons().claim_shape_work(100_000).await?;
    assert!(
        !after_first.iter().any(|w| w.lesson_id == id),
        "a failed lesson waits out its backoff"
    );
    assert!(
        !f.handle
            .lessons()
            .record_shape_failure(id, "second")
            .await?
    );
    assert!(f.handle.lessons().record_shape_failure(id, "third").await?);
    let finding = f.handle.lessons().shape_finding(id).await?;
    assert_eq!(finding.and_then(|r| r.resolution), Some("kept".to_owned()));
    assert!(!f.handle.lessons().lesson_has_open_shape_finding(id).await?);
    let claimed = f
        .handle
        .lessons()
        .claim_raw_lessons_for_consolidation(f.project_id, 10)
        .await?;
    assert!(claimed.iter().any(|l| l.id == id));
    Ok(())
}
