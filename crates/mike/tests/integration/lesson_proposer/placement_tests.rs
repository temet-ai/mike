use mike::lesson_proposer::dedupe::Embedded;
use mike::lesson_proposer::placement::{Home, Placement};
use mike::lesson_proposer::prompt::ProposedLesson;
use mike::lesson_proposer::relate::{WriteAction, WriteTimeVerdict};
use mike::lesson_proposer::write::{Decision, WriteOutcome, decide_and_write};
use mike::lesson_proposer::{ProposerSettings, run_tick};
use mike::store::persistence::lessons::NewLessonWrite;
use mike::store::persistence::lessons_repo::LessonsPersist as _;
use serde_json::json;

use crate::turn_facts::{Fixture, fixture};

use super::seed_corrected_turn;
use super::support::{PLACEMENT_FAILS, Relate, Rig, Scripted, Shape};

const RULE: &str = "Run the tests and read their exit status before claiming they pass.";

fn scripted(home: &'static str) -> Scripted {
    Scripted {
        relate: Relate::Update,
        shape: Shape::TwoRules,
        home,
    }
}

fn proposed() -> anyhow::Result<ProposedLesson> {
    Ok(serde_json::from_value(json!({
        "rule": RULE,
        "kind": "correction",
        "generalises": true,
        "reason": "the operator caught a false claim",
    }))?)
}

fn embedded() -> Embedded {
    Embedded {
        vector: vec![0.01_f32; 768],
        model: "test-embedder".to_owned(),
    }
}

fn verdict(action: WriteAction, home: Home, covered_by: Option<&str>) -> WriteTimeVerdict {
    WriteTimeVerdict {
        action,
        relations: Vec::new(),
        placement: Placement {
            home,
            covered_by: covered_by.map(str::to_owned),
            reason: "scripted".to_owned(),
        },
        notes: Vec::new(),
    }
}

async fn unplaced_lesson(f: &Fixture, text: &str, topic: &str) -> anyhow::Result<i64> {
    let row = f
        .handle
        .lessons()
        .create_lesson_with_topic(NewLessonWrite {
            project_id: f.project_id,
            kind: "convention",
            text,
            topic: Some(topic),
            source_turn_id: None,
            agent_id: None,
            session_db_id: None,
            request_id: None,
        })
        .await?;
    sqlx::query(
        "UPDATE memory.lessons SET stage_id = (SELECT id FROM memory.lesson_stages WHERE code = 'daily')
          WHERE id = $1",
    )
    .bind(row.id)
    .execute(f.handle.pool())
    .await?;
    Ok(row.id)
}

async fn placement_of(f: &Fixture, id: i64) -> anyhow::Result<Option<String>> {
    Ok(sqlx::query_scalar(
        "SELECT lp.code FROM memory.lessons l
           LEFT JOIN memory.lesson_placements lp ON lp.id = l.placement_id WHERE l.id = $1",
    )
    .bind(id)
    .fetch_one(f.handle.pool())
    .await?)
}

#[tokio::test]
async fn a_created_lesson_carries_the_placement_the_verdict_chose() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let candidate = seed_corrected_turn(&f, 52_000).await?;
    let empty = json!([]);
    let chosen = verdict(WriteAction::Store, Home::Skill, Some("deploy/SKILL.md"));
    let decision = Decision {
        candidate: &candidate,
        proposed: &proposed()?,
        embedding: Some(&embedded()),
        verdict: Some(&chosen),
        model: "test-model",
        cost_usd: None,
        generation_id: None,
        similar: &empty,
    };
    let WriteOutcome::Created { lesson_id, .. } =
        decide_and_write(&f.handle, &ProposerSettings::default(), &decision).await?
    else {
        anyhow::bail!("expected a created lesson");
    };
    let stored: (String, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT lp.code, l.placement_covered_by, l.placement_reason
           FROM memory.lessons l JOIN memory.lesson_placements lp ON lp.id = l.placement_id
          WHERE l.id = $1",
    )
    .bind(lesson_id)
    .fetch_one(f.handle.pool())
    .await?;
    assert_eq!(
        stored,
        (
            "skill".to_owned(),
            Some("deploy/SKILL.md".to_owned()),
            Some("scripted".to_owned())
        )
    );
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_lesson_placed_nowhere_is_not_stored_and_the_evidence_names_the_document()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let candidate = seed_corrected_turn(&f, 52_100).await?;
    let empty = json!([]);
    let covered = verdict(
        WriteAction::Discard {
            covered_by: ".claude/rules/testing.md".to_owned(),
        },
        Home::Nowhere,
        Some(".claude/rules/testing.md"),
    );
    let decision = Decision {
        candidate: &candidate,
        proposed: &proposed()?,
        embedding: Some(&embedded()),
        verdict: Some(&covered),
        model: "test-model",
        cost_usd: None,
        generation_id: None,
        similar: &empty,
    };
    let outcome = decide_and_write(&f.handle, &ProposerSettings::default(), &decision).await?;
    assert_eq!(outcome, WriteOutcome::EvidenceOnly);
    let lessons: i64 =
        sqlx::query_scalar("SELECT count(*) FROM memory.lessons WHERE project_id = $1")
            .bind(f.project_id)
            .fetch_one(f.handle.pool())
            .await?;
    assert_eq!(lessons, 0);
    let reason: String = sqlx::query_scalar(
        "SELECT reason FROM memory.lesson_proposal_evidence WHERE verdict_id = $1",
    )
    .bind(candidate.verdict_id)
    .fetch_one(f.handle.pool())
    .await?;
    assert!(reason.contains(".claude/rules/testing.md"), "{reason}");
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn the_placement_pass_places_lessons_written_without_one() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let first = unplaced_lesson(&f, RULE, "testing").await?;
    let second = unplaced_lesson(
        &f,
        "Never hand a decision over without showing it.",
        "reports",
    )
    .await?;
    let rig = Rig::new(scripted("rule"), ProposerSettings::default()).await?;

    let report = run_tick(&rig.deps(&f.handle)).await;

    assert_eq!(report.placement.placed, 2, "{report:?}");
    assert_eq!(placement_of(&f, first).await?.as_deref(), Some("rule"));
    assert_eq!(placement_of(&f, second).await?.as_deref(), Some("rule"));
    let second_tick = run_tick(&rig.deps(&f.handle)).await;
    assert_eq!(second_tick.placement.placed, 0);
    assert_eq!(
        rig.llm_calls("You decide where one stored project lesson belongs")
            .await,
        2
    );
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_failed_placement_waits_and_is_not_offered_again_at_once() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let id = unplaced_lesson(&f, RULE, "testing").await?;
    let rig = Rig::new(scripted(PLACEMENT_FAILS), ProposerSettings::default()).await?;

    let first = run_tick(&rig.deps(&f.handle)).await;
    let second = run_tick(&rig.deps(&f.handle)).await;

    assert_eq!((first.placement.failed, second.placement.failed), (1, 0));
    assert_eq!(placement_of(&f, id).await?, None);
    let attempts: i32 = sqlx::query_scalar(
        "SELECT attempts FROM memory.lesson_placement_failures WHERE lesson_id = $1",
    )
    .bind(id)
    .fetch_one(f.handle.pool())
    .await?;
    assert_eq!(attempts, 1);
    assert_eq!(
        rig.llm_calls("You decide where one stored project lesson belongs")
            .await,
        1
    );
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn overlapping_skill_topics_are_merged_onto_one_on_the_tick() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    for (text, topic) in [
        (
            "Restart with start-proxy.sh after a deploy.",
            "deploying-the-proxy",
        ),
        ("Deploy only from the main checkout.", "deploying-the-proxy"),
        (
            "Check the status box after start-proxy.sh.",
            "proxy-deploy-steps",
        ),
        (
            "Never deploy while a fleet is running.",
            "proxy-deploy-steps",
        ),
        (
            "Pass --during-a-fleet only when told.",
            "proxy-deploy-steps",
        ),
    ] {
        let id = unplaced_lesson(&f, text, topic).await?;
        sqlx::query(
            "UPDATE memory.lessons
                SET placement_id = (SELECT id FROM memory.lesson_placements WHERE code = 'skill'),
                    placed_at = now()
              WHERE id = $1",
        )
        .bind(id)
        .execute(f.handle.pool())
        .await?;
    }
    let rig = Rig::new(scripted("skill"), ProposerSettings::default()).await?;

    let report = run_tick(&rig.deps(&f.handle)).await;

    assert_eq!(report.placement.placed, 0, "{report:?}");
    assert_eq!(
        (report.purpose.merged_topics, report.purpose.moved_lessons),
        (1, 2),
        "{report:?}"
    );
    let topics: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT topic FROM memory.lessons WHERE project_id = $1 ORDER BY 1",
    )
    .bind(f.project_id)
    .fetch_all(f.handle.pool())
    .await?;
    assert_eq!(topics, vec!["proxy-deploy-steps".to_owned()]);
    f.db.cleanup().await?;
    Ok(())
}
