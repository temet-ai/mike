use mike::lesson_proposer::{ProposerSettings, run_tick};

use crate::turn_facts::{Fixture, fixture};

use super::seed_corrected_turn;
use super::support::{REPLACEMENT, Relate, Rig, Scripted, Shape};

const EXISTING: &str = "Run the tests before claiming they pass.";

fn scripted(relate: Relate) -> Scripted {
    Scripted {
        relate,
        shape: Shape::TwoRules,
        home: "memory",
    }
}

fn settings(auto_promote_min_evidence: i64) -> ProposerSettings {
    ProposerSettings {
        auto_promote_min_evidence,
        ..ProposerSettings::default()
    }
}

async fn existing_lesson(f: &Fixture, source: &str, stage: &str) -> anyhow::Result<i64> {
    let mut vector = vec!["0"; 768];
    vector[0] = "1";
    let literal = format!("[{}]", vector.join(","));
    Ok(sqlx::query_scalar(
        "INSERT INTO memory.lessons
            (project_id, kind_id, text, text_sha256, stage_id, source_id,
             embedding, embedding_model, embedded_at)
         SELECT $1, lk.id, $2, decode('01', 'hex'), ls.id, src.id, $5::vector, 'test-embedder', now()
           FROM memory.lesson_kinds lk, memory.lesson_stages ls, memory.lesson_sources src
          WHERE lk.code = 'correction' AND ls.code = $3 AND src.code = $4
         RETURNING id",
    )
    .bind(f.project_id)
    .bind(EXISTING)
    .bind(stage)
    .bind(source)
    .bind(literal)
    .fetch_one(f.handle.pool())
    .await?)
}

async fn relations_to(f: &Fixture, to: i64) -> anyhow::Result<Vec<(i64, String, String, String)>> {
    Ok(sqlx::query_as(
        "SELECT r.from_lesson_id, k.code, s.code, m.code
           FROM memory.lesson_relations r
           JOIN memory.lesson_relation_kinds k ON k.id = r.kind_id
           JOIN memory.lesson_relation_statuses s ON s.id = r.status_id
           JOIN memory.lesson_relation_methods m ON m.id = r.method_id
          WHERE r.to_lesson_id = $1 ORDER BY r.id",
    )
    .bind(to)
    .fetch_all(f.handle.pool())
    .await?)
}

async fn stage_and_text(f: &Fixture, id: i64) -> anyhow::Result<(String, String)> {
    Ok(sqlx::query_as(
        "SELECT ls.code, l.text FROM memory.lessons l
           JOIN memory.lesson_stages ls ON ls.id = l.stage_id WHERE l.id = $1",
    )
    .bind(id)
    .fetch_one(f.handle.pool())
    .await?)
}

#[tokio::test]
async fn an_update_decision_stores_the_replacement_and_proposes_a_supersedes_relation()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    seed_corrected_turn(&f, 51_000).await?;
    let existing = existing_lesson(&f, "judge", "long_term").await?;
    let rig = Rig::new(scripted(Relate::Update), settings(5)).await?;

    let report = run_tick(&rig.deps(&f.handle)).await;

    assert_eq!((report.created, report.errors), (1, 0), "{report:?}");
    let relations = relations_to(&f, existing).await?;
    assert_eq!(relations.len(), 1, "{relations:?}");
    let (new_id, kind, status, method) = relations[0].clone();
    assert_eq!(
        (kind.as_str(), status.as_str(), method.as_str()),
        ("supersedes", "proposed", "write_time")
    );
    let (stage, text) = stage_and_text(&f, new_id).await?;
    assert_eq!((stage.as_str(), text.as_str()), ("raw", REPLACEMENT));
    assert_eq!(stage_and_text(&f, existing).await?.0, "long_term");
    assert_eq!(
        rig.llm_calls("You decide how one new lesson relates").await,
        1
    );

    f.handle.confirm_judge_only_write_time_relations().await?;
    f.handle.apply_confirmed_lesson_relations().await?;
    assert_eq!(
        stage_and_text(&f, existing).await?.0,
        "long_term",
        "the replaced lesson stays until the replacement is promoted"
    );
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_promoted_judge_replacement_archives_the_replaced_lesson_at_the_nightly_run()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    seed_corrected_turn(&f, 51_100).await?;
    let existing = existing_lesson(&f, "judge", "long_term").await?;
    let rig = Rig::new(scripted(Relate::Update), settings(1)).await?;

    let report = run_tick(&rig.deps(&f.handle)).await;
    assert_eq!((report.created, report.promoted), (1, 1), "{report:?}");

    assert_eq!(f.handle.confirm_judge_only_write_time_relations().await?, 1);
    assert_eq!(f.handle.apply_confirmed_lesson_relations().await?, 1);
    let relations = relations_to(&f, existing).await?;
    assert_eq!(relations[0].2, "confirmed");
    let archived: Option<String> = sqlx::query_scalar(
        "SELECT ar.code FROM memory.lessons l
           LEFT JOIN memory.lesson_archived_reasons ar ON ar.id = l.archived_reason_id
          WHERE l.id = $1",
    )
    .bind(existing)
    .fetch_one(f.handle.pool())
    .await?;
    assert_eq!(archived.as_deref(), Some("superseded"));
    let project_path: String =
        sqlx::query_scalar("SELECT project_path FROM projects.projects WHERE id = $1")
            .bind(f.project_id)
            .fetch_one(f.handle.pool())
            .await?;
    let injectable: Vec<String> = f
        .handle
        .list_injectable_lessons(&project_path)
        .await?
        .into_iter()
        .map(|lesson| lesson.text)
        .collect();
    assert_eq!(injectable, vec![REPLACEMENT.to_owned()]);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn an_update_of_an_operator_lesson_waits_for_the_operator() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    seed_corrected_turn(&f, 51_200).await?;
    let existing = existing_lesson(&f, "operator", "long_term").await?;
    let rig = Rig::new(scripted(Relate::Update), settings(1)).await?;

    let report = run_tick(&rig.deps(&f.handle)).await;
    assert_eq!(report.created, 1, "{report:?}");

    assert_eq!(f.handle.confirm_judge_only_write_time_relations().await?, 0);
    assert_eq!(f.handle.apply_confirmed_lesson_relations().await?, 0);
    assert_eq!(relations_to(&f, existing).await?[0].2, "proposed");
    assert_eq!(stage_and_text(&f, existing).await?.0, "long_term");
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_skip_decision_stores_no_active_lesson_but_keeps_the_text_and_a_recurs_relation()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    seed_corrected_turn(&f, 51_300).await?;
    let existing = existing_lesson(&f, "judge", "long_term").await?;
    let rig = Rig::new(scripted(Relate::Skip), settings(5)).await?;

    let report = run_tick(&rig.deps(&f.handle)).await;

    assert_eq!((report.reinforced, report.created), (1, 0), "{report:?}");
    let relations = relations_to(&f, existing).await?;
    assert_eq!(relations.len(), 1);
    assert_eq!(relations[0].1, "recurs");
    let (reason, text): (String, String) = sqlx::query_as(
        "SELECT ar.code, l.text FROM memory.lessons l
           JOIN memory.lesson_archived_reasons ar ON ar.id = l.archived_reason_id
          WHERE l.id = $1",
    )
    .bind(relations[0].0)
    .fetch_one(f.handle.pool())
    .await?;
    assert_eq!(reason, "duplicate");
    assert_eq!(text, "Always run the tests before claiming they pass.");
    let evidence: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM memory.lesson_proposal_evidence WHERE lesson_id = $1",
    )
    .bind(existing)
    .fetch_one(f.handle.pool())
    .await?;
    assert_eq!(evidence, 1);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_failed_decision_call_writes_nothing_and_leaves_the_proposal_pending()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    seed_corrected_turn(&f, 51_400).await?;
    let existing = existing_lesson(&f, "judge", "long_term").await?;
    let rig = Rig::new(scripted(Relate::Fail), settings(5)).await?;

    let report = run_tick(&rig.deps(&f.handle)).await;

    assert_eq!(
        (report.created, report.reinforced, report.errors),
        (0, 0, 1),
        "{report:?}"
    );
    assert!(relations_to(&f, existing).await?.is_empty());
    let lessons: i64 =
        sqlx::query_scalar("SELECT count(*) FROM memory.lessons WHERE project_id = $1")
            .bind(f.project_id)
            .fetch_one(f.handle.pool())
            .await?;
    assert_eq!(lessons, 1, "only the pre-existing lesson");
    let evidence: i64 = sqlx::query_scalar("SELECT count(*) FROM memory.lesson_proposal_evidence")
        .fetch_one(f.handle.pool())
        .await?;
    assert_eq!(evidence, 0);
    let jobs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM memory.lesson_proposal_jobs WHERE status = 'awaiting_embedding'",
    )
    .fetch_one(f.handle.pool())
    .await?;
    assert_eq!(jobs, 1, "the saved proposal is retained for the retry");
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn with_no_near_lesson_the_new_one_is_stored_by_one_call_that_decides_its_placement()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    seed_corrected_turn(&f, 51_500).await?;
    let mut scripted = scripted(Relate::Update);
    scripted.home = "skill";
    let rig = Rig::new(scripted, settings(5)).await?;

    let report = run_tick(&rig.deps(&f.handle)).await;

    assert_eq!(report.created, 1, "{report:?}");
    assert_eq!(
        rig.llm_calls("You decide how one new lesson relates").await,
        1
    );
    let placed: Vec<(String, String)> = sqlx::query_as(
        "SELECT lp.code, l.placement_reason
           FROM memory.lessons l JOIN memory.lesson_placements lp ON lp.id = l.placement_id
          WHERE l.project_id = $1",
    )
    .bind(f.project_id)
    .fetch_all(f.handle.pool())
    .await?;
    assert_eq!(
        placed,
        vec![("skill".to_owned(), "scripted placement".to_owned())]
    );
    f.db.cleanup().await?;
    Ok(())
}
