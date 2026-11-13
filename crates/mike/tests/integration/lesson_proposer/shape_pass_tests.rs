use mike::lesson_proposer::{ProposerSettings, run_tick};
use mike::lesson_shape::assess;
use mike::store::persistence::lessons::NewLessonWrite;
use mike::store::persistence::lessons_repo::LessonsPersist as _;

use crate::turn_facts::{Fixture, fixture};

use super::support::{REWRITTEN_A, REWRITTEN_B, Relate, Rig, Scripted, Shape};

const LONG: &str = "Verify every claim against the running system before reporting it. ";

fn scripted(shape: Shape) -> Scripted {
    Scripted {
        relate: Relate::Fail,
        shape,
        home: "memory",
    }
}

async fn flagged_lesson(
    f: &Fixture,
    text: &str,
    topic: Option<&str>,
    stage: &str,
) -> anyhow::Result<i64> {
    let assessment = assess(text, topic, 500);
    let row = f
        .handle
        .lessons()
        .create_lesson_with_shape(
            NewLessonWrite {
                project_id: f.project_id,
                kind: "convention",
                text,
                topic,
                source_turn_id: None,
                agent_id: None,
                session_db_id: Some(f.session_id),
                request_id: None,
            },
            &assessment,
            true,
        )
        .await?;
    sqlx::query(
        "UPDATE memory.lessons l SET stage_id = s.id FROM memory.lesson_stages s
          WHERE s.code = $2 AND l.id = $1",
    )
    .bind(row.id)
    .bind(stage)
    .execute(f.handle.pool())
    .await?;
    Ok(row.id)
}

async fn archived_reason(f: &Fixture, id: i64) -> anyhow::Result<Option<String>> {
    Ok(sqlx::query_scalar(
        "SELECT ar.code FROM memory.lessons l
           LEFT JOIN memory.lesson_archived_reasons ar ON ar.id = l.archived_reason_id
          WHERE l.id = $1",
    )
    .bind(id)
    .fetch_one(f.handle.pool())
    .await?)
}

async fn active_texts(f: &Fixture) -> anyhow::Result<Vec<String>> {
    Ok(sqlx::query_scalar(
        "SELECT text FROM memory.lessons
          WHERE project_id = $1 AND archived_at IS NULL ORDER BY id",
    )
    .bind(f.project_id)
    .fetch_all(f.handle.pool())
    .await?)
}

#[tokio::test]
async fn an_over_length_long_term_lesson_is_rewritten_into_rules_on_the_tick() -> anyhow::Result<()>
{
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let original = LONG.repeat(14);
    let id = flagged_lesson(&f, &original, Some("verification"), "long_term").await?;
    let rig = Rig::new(scripted(Shape::TwoRules), ProposerSettings::default()).await?;

    let report = run_tick(&rig.deps(&f.handle)).await;

    assert_eq!(
        (report.shape.rewritten, report.shape.replacements),
        (1, 2),
        "{report:?}"
    );
    assert_eq!(
        active_texts(&f).await?,
        vec![REWRITTEN_A.to_owned(), REWRITTEN_B.to_owned()]
    );
    assert_eq!(
        archived_reason(&f, id).await?.as_deref(),
        Some("superseded")
    );
    let kept: String = sqlx::query_scalar("SELECT text FROM memory.lessons WHERE id = $1")
        .bind(id)
        .fetch_one(f.handle.pool())
        .await?;
    assert_eq!(kept, original);
    let stages: Vec<String> = sqlx::query_scalar(
        "SELECT ls.code FROM memory.lessons l JOIN memory.lesson_stages ls ON ls.id = l.stage_id
          WHERE l.project_id = $1 AND l.archived_at IS NULL",
    )
    .bind(f.project_id)
    .fetch_all(f.handle.pool())
    .await?;
    assert!(stages.iter().all(|s| s == "long_term"), "{stages:?}");
    let embedded: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM memory.lessons
          WHERE project_id = $1 AND archived_at IS NULL AND embedding IS NOT NULL",
    )
    .bind(f.project_id)
    .fetch_one(f.handle.pool())
    .await?;
    assert_eq!(
        embedded, 2,
        "rewrites of injected lessons are embedded at once"
    );
    assert_eq!(rig.llm_calls("You rewrite one stored lesson").await, 1);

    let second = run_tick(&rig.deps(&f.handle)).await;
    assert_eq!(second.shape.rewritten, 0);
    assert_eq!(rig.llm_calls("You rewrite one stored lesson").await, 1);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_verifier_note_is_retired_without_a_model_call_and_handed_over_to_its_session()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let text = "Left unmet when session 1bd1159e-57bb-46ee-955b-2ea18fa6d5e2 ended stopped-unverified: 1. Worker + observer each wrote a file.";
    let id = flagged_lesson(&f, text, Some("verifier:unmet"), "long_term").await?;
    let rig = Rig::new(scripted(Shape::TwoRules), ProposerSettings::default()).await?;

    let report = run_tick(&rig.deps(&f.handle)).await;

    assert_eq!(report.shape.retired, 1, "{report:?}");
    assert_eq!(
        archived_reason(&f, id).await?.as_deref(),
        Some("not_a_lesson")
    );
    assert_eq!(rig.llm_calls("You rewrite one stored lesson").await, 0);
    let notes: Vec<(String, String, Vec<String>)> = sqlx::query_as(
        "SELECT kind, body, tags FROM workspace.session_notes WHERE session_id = $1",
    )
    .bind(f.session_id)
    .fetch_all(f.handle.pool())
    .await?;
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].0, "handover");
    assert!(notes[0].1.contains(text));
    assert!(notes[0].2.contains(&format!("lesson:{id}")));
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_status_text_with_no_rule_in_it_is_retired_but_an_over_length_one_is_not()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let status = flagged_lesson(
        &f,
        "Done. The gate passed and the work was merged.",
        None,
        "long_term",
    )
    .await?;
    let long = flagged_lesson(&f, &LONG.repeat(14), None, "long_term").await?;
    let rig = Rig::new(scripted(Shape::NoRule), ProposerSettings::default()).await?;

    let report = run_tick(&rig.deps(&f.handle)).await;

    assert_eq!(report.shape.retired, 1, "{report:?}");
    assert_eq!(report.shape.failed, 1);
    assert_eq!(
        archived_reason(&f, status).await?.as_deref(),
        Some("not_a_lesson")
    );
    assert_eq!(archived_reason(&f, long).await?, None);
    let finding = f.handle.lessons().shape_finding(long).await?;
    assert_eq!(finding.map(|r| r.attempts), Some(1));
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_rewrite_that_is_still_too_long_is_refused_and_the_reason_names_the_limit()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let id = flagged_lesson(&f, &LONG.repeat(14), None, "daily").await?;
    let rig = Rig::new(scripted(Shape::TooLong), ProposerSettings::default()).await?;

    let report = run_tick(&rig.deps(&f.handle)).await;

    assert_eq!(
        (report.shape.rewritten, report.shape.failed),
        (0, 1),
        "{report:?}"
    );
    let error: Option<String> = sqlx::query_scalar(
        "SELECT last_error FROM memory.lesson_shape_findings WHERE lesson_id = $1",
    )
    .bind(id)
    .fetch_one(f.handle.pool())
    .await?;
    let error = error.unwrap_or_default();
    assert!(error.contains("limit is 500"), "{error}");
    assert_eq!(active_texts(&f).await?.len(), 1);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn the_operator_limit_setting_changes_what_is_flagged() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let text = LONG.repeat(5);
    let lesson = f
        .handle
        .lessons()
        .create_lesson_with_topic(NewLessonWrite {
            project_id: f.project_id,
            kind: "convention",
            text: &text,
            topic: None,
            source_turn_id: None,
            agent_id: None,
            session_db_id: None,
            request_id: None,
        })
        .await?;
    let settings = ProposerSettings {
        max_chars: 200,
        ..ProposerSettings::default()
    };
    let rig = Rig::new(scripted(Shape::TwoRules), settings).await?;

    let report = run_tick(&rig.deps(&f.handle)).await;

    assert!(report.shape.flagged >= 1, "{report:?}");
    assert_eq!(
        f.handle
            .lessons()
            .shape_finding(lesson.id)
            .await?
            .map(|r| r.shape),
        Some("over_length".to_owned())
    );
    f.db.cleanup().await?;
    Ok(())
}
