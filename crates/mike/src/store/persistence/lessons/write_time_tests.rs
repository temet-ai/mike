use crate::store::persistence::lessons_repo::LessonsPersist as _;
use serde_json::json;

use super::{OTHER_SESSION, fixture, no_evidence, seed_verdict, unit_embedding};
use crate::store::persistence::lessons::LessonInsertRow;
use crate::store::persistence::lessons::NewLessonWrite;
use crate::store::persistence::lessons::proposals::NewCandidateLesson;
use crate::store::persistence::lessons::write_time::PlannedRelation;

struct Pair {
    fx: super::Fixture,
    old: i64,
    new: i64,
}

fn relation(to: i64, kind: &str) -> PlannedRelation {
    PlannedRelation {
        to_lesson_id: to,
        kind: kind.to_owned(),
        distance: 0.05,
        rationale: "the new text restates the old one".to_owned(),
        evidence: json!({"decision": kind}),
    }
}

async fn session_of_turn(fx: &super::Fixture, turn_id: i64) -> anyhow::Result<i64> {
    Ok(
        sqlx::query_scalar("SELECT session_id FROM memory.operator_turns WHERE id = $1")
            .bind(turn_id)
            .fetch_one(fx.handle.pool())
            .await?,
    )
}

async fn judge_lesson(
    fx: &super::Fixture,
    verdict_id: i64,
    turn_id: i64,
    text: &str,
    relations: &[PlannedRelation],
) -> anyhow::Result<i64> {
    let session_db_id = session_of_turn(fx, turn_id).await?;
    let embedding = unit_embedding();
    Ok(fx
        .handle
        .lessons()
        .create_candidate_lesson(
            NewCandidateLesson {
                project_id: fx.project_id,
                kind: "correction",
                text,
                source_turn_id: turn_id,
                session_db_id,
                request_id: None,
                embedding: &embedding,
                embedding_model: "test-embedder",
                shape: None,
                relations,
                placement: None,
            },
            no_evidence(verdict_id, turn_id, &json!([])),
        )
        .await?)
}

async fn set_stage(fx: &super::Fixture, id: i64, stage: &str) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE memory.lessons l SET stage_id = s.id FROM memory.lesson_stages s
          WHERE s.code = $2 AND l.id = $1",
    )
    .bind(id)
    .bind(stage)
    .execute(fx.handle.pool())
    .await?;
    Ok(())
}

async fn pair_with_relation(kind: &str, old_is_operator: bool) -> anyhow::Result<Option<Pair>> {
    let Some(fx) = fixture(0.9, "resolved", true).await? else {
        return Ok(None);
    };
    let old = if old_is_operator {
        let row: LessonInsertRow = fx
            .handle
            .lessons()
            .create_lesson_with_topic(NewLessonWrite {
                project_id: fx.project_id,
                kind: "convention",
                text: "Run the tests before claiming they pass.",
                topic: None,
                source_turn_id: None,
                agent_id: None,
                session_db_id: None,
                request_id: None,
            })
            .await?;
        set_stage(&fx, row.id, "long_term").await?;
        row.id
    } else {
        let id = judge_lesson(
            &fx,
            fx.verdict_id,
            fx.turn_id,
            "Run the tests before claiming they pass.",
            &[],
        )
        .await?;
        fx.handle
            .lessons()
            .approve_lesson_candidate(id, None, "judge")
            .await?;
        id
    };
    let second = seed_verdict(
        &fx.handle,
        OTHER_SESSION,
        fx.project_id,
        0.9,
        "resolved",
        true,
    )
    .await?;
    let new = judge_lesson(
        &fx,
        second.verdict_id,
        second.turn_id,
        "Run the tests and read their exit status before claiming they pass.",
        &[relation(old, kind)],
    )
    .await?;
    Ok(Some(Pair { fx, old, new }))
}

async fn relation_row(
    fx: &super::Fixture,
    from: i64,
    to: i64,
) -> anyhow::Result<(String, String, String, Option<String>)> {
    Ok(sqlx::query_as(
        "SELECT k.code, s.code, m.code, r.decided_by
           FROM memory.lesson_relations r
           JOIN memory.lesson_relation_kinds k ON k.id = r.kind_id
           JOIN memory.lesson_relation_statuses s ON s.id = r.status_id
           JOIN memory.lesson_relation_methods m ON m.id = r.method_id
          WHERE r.from_lesson_id = $1 AND r.to_lesson_id = $2",
    )
    .bind(from)
    .bind(to)
    .fetch_one(fx.handle.pool())
    .await?)
}

async fn stage_of(fx: &super::Fixture, id: i64) -> anyhow::Result<(String, Option<String>)> {
    Ok(sqlx::query_as(
        "SELECT ls.code, ar.code FROM memory.lessons l
           JOIN memory.lesson_stages ls ON ls.id = l.stage_id
           LEFT JOIN memory.lesson_archived_reasons ar ON ar.id = l.archived_reason_id
          WHERE l.id = $1",
    )
    .bind(id)
    .fetch_one(fx.handle.pool())
    .await?)
}

#[tokio::test]
async fn rewriting_a_judge_candidate_moves_its_evidence_to_the_first_rule() -> anyhow::Result<()> {
    use crate::store::persistence::lessons::shape_apply::{RewrittenRule, ShapeRewrite};
    let Some(fx) = fixture(0.9, "resolved", true).await? else {
        return Ok(());
    };
    let original = judge_lesson(
        &fx,
        fx.verdict_id,
        fx.turn_id,
        "Run the tests before claiming they pass.",
        &[],
    )
    .await?;
    let rules = [
        RewrittenRule {
            text: "Run the tests before claiming they pass.".to_owned(),
            embedding: None,
        },
        RewrittenRule {
            text: "Read the exit status of every command you report on.".to_owned(),
            embedding: None,
        },
    ];
    let ids = fx
        .handle
        .lessons()
        .rewrite_lesson_into_rules(&ShapeRewrite {
            lesson_id: original,
            rules: &rules,
            model: "test-model",
            original_chars: 40,
            rationale: "two rules",
        })
        .await?
        .unwrap_or_default();
    assert_eq!(ids.len(), 2);
    assert_eq!(fx.handle.lessons().count_lesson_evidence(ids[0]).await?, 1);
    assert_eq!(fx.handle.lessons().count_lesson_evidence(ids[1]).await?, 0);
    assert_eq!(
        fx.handle.lessons().count_lesson_evidence(original).await?,
        0
    );
    let (stage, reason) = stage_of(&fx, original).await?;
    assert_eq!(
        (stage.as_str(), reason.as_deref()),
        ("archived", Some("superseded"))
    );
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn creating_with_planned_relations_writes_proposed_write_time_relations() -> anyhow::Result<()>
{
    let Some(p) = pair_with_relation("supersedes", false).await? else {
        return Ok(());
    };
    let (kind, status, method, decided_by) = relation_row(&p.fx, p.new, p.old).await?;
    assert_eq!(
        (kind.as_str(), status.as_str(), method.as_str(), decided_by),
        ("supersedes", "proposed", "write_time", None)
    );
    let (model, distance, evidence): (String, f64, serde_json::Value) = sqlx::query_as(
        "SELECT proposer_model, candidate_distance, evidence_json
           FROM memory.lesson_relations WHERE from_lesson_id = $1",
    )
    .bind(p.new)
    .fetch_one(p.fx.handle.pool())
    .await?;
    assert_eq!(model, "test-model");
    assert!((distance - 0.05).abs() < 1e-9);
    assert_eq!(evidence["decision"], "supersedes");
    p.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn promotion_evidence_counts_the_corrections_behind_the_replaced_lesson() -> anyhow::Result<()>
{
    let Some(p) = pair_with_relation("supersedes", false).await? else {
        return Ok(());
    };
    assert_eq!(p.fx.handle.lessons().count_lesson_evidence(p.new).await?, 1);
    assert_eq!(
        p.fx.handle
            .lessons()
            .count_promotion_evidence(p.new)
            .await?,
        2
    );
    assert_eq!(
        p.fx.handle
            .lessons()
            .count_promotion_evidence(p.old)
            .await?,
        1
    );
    p.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn judge_only_relations_confirm_after_the_replacement_is_promoted() -> anyhow::Result<()> {
    let Some(p) = pair_with_relation("supersedes", false).await? else {
        return Ok(());
    };
    assert_eq!(
        p.fx.handle
            .lessons()
            .confirm_judge_only_write_time_relations()
            .await?,
        0
    );
    assert_eq!(relation_row(&p.fx, p.new, p.old).await?.1, "proposed");

    p.fx.handle
        .lessons()
        .approve_lesson_candidate(p.new, None, "judge")
        .await?;
    assert_eq!(
        p.fx.handle
            .lessons()
            .confirm_judge_only_write_time_relations()
            .await?,
        1
    );
    let (_, status, _, decided_by) = relation_row(&p.fx, p.new, p.old).await?;
    assert_eq!(status, "confirmed");
    assert_eq!(decided_by.as_deref(), Some("judge_reinforcement"));
    p.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_relation_that_touches_an_operator_lesson_waits_for_the_operator() -> anyhow::Result<()> {
    let Some(p) = pair_with_relation("supersedes", true).await? else {
        return Ok(());
    };
    p.fx.handle
        .lessons()
        .approve_lesson_candidate(p.new, None, "judge")
        .await?;
    assert_eq!(
        p.fx.handle
            .lessons()
            .confirm_judge_only_write_time_relations()
            .await?,
        0
    );
    assert_eq!(relation_row(&p.fx, p.new, p.old).await?.1, "proposed");
    assert_eq!(
        p.fx.handle
            .lessons()
            .apply_confirmed_lesson_relations()
            .await?,
        0
    );
    assert_eq!(stage_of(&p.fx, p.old).await?.0, "long_term");

    let id: i64 = sqlx::query_scalar(
        "SELECT id FROM memory.lesson_relations WHERE from_lesson_id = $1 AND to_lesson_id = $2",
    )
    .bind(p.new)
    .bind(p.old)
    .fetch_one(p.fx.handle.pool())
    .await?;
    p.fx.handle
        .lessons()
        .confirm_relation(id, "operator")
        .await?;
    assert_eq!(
        p.fx.handle
            .lessons()
            .apply_confirmed_lesson_relations()
            .await?,
        1
    );
    let (stage, reason) = stage_of(&p.fx, p.old).await?;
    assert_eq!(
        (stage.as_str(), reason.as_deref()),
        ("archived", Some("superseded"))
    );
    p.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_contradicts_relation_is_never_confirmed_or_applied() -> anyhow::Result<()> {
    let Some(p) = pair_with_relation("contradicts", false).await? else {
        return Ok(());
    };
    p.fx.handle
        .lessons()
        .approve_lesson_candidate(p.new, None, "judge")
        .await?;
    assert_eq!(
        p.fx.handle
            .lessons()
            .confirm_judge_only_write_time_relations()
            .await?,
        0
    );
    assert_eq!(
        p.fx.handle
            .lessons()
            .apply_confirmed_lesson_relations()
            .await?,
        0
    );
    assert_eq!(relation_row(&p.fx, p.new, p.old).await?.1, "proposed");
    assert_eq!(stage_of(&p.fx, p.old).await?.0, "daily");
    p.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn applying_waits_while_the_replacement_is_still_a_raw_judge_candidate() -> anyhow::Result<()>
{
    let Some(p) = pair_with_relation("supersedes", false).await? else {
        return Ok(());
    };
    sqlx::query(
        "UPDATE memory.lesson_relations r SET status_id = s.id
           FROM memory.lesson_relation_statuses s WHERE s.code = 'confirmed' AND r.from_lesson_id = $1",
    )
    .bind(p.new)
    .execute(p.fx.handle.pool())
    .await?;
    assert_eq!(
        p.fx.handle
            .lessons()
            .apply_confirmed_lesson_relations()
            .await?,
        0
    );
    assert_eq!(stage_of(&p.fx, p.old).await?.0, "daily");
    p.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn applying_archives_the_old_lesson_moves_evidence_and_inherits_relations()
-> anyhow::Result<()> {
    let Some(p) = pair_with_relation("refines", false).await? else {
        return Ok(());
    };
    let other: i64 = sqlx::query_scalar(
        "INSERT INTO memory.lessons (project_id, kind_id, text, text_sha256, stage_id, source_id)
         SELECT $1, lk.id, 'An unrelated lesson that contradicts the old one.', decode('00', 'hex'), ls.id, src.id
           FROM memory.lesson_kinds lk, memory.lesson_stages ls, memory.lesson_sources src
          WHERE lk.code = 'correction' AND ls.code = 'long_term' AND src.code = 'judge'
         RETURNING id",
    )
    .bind(p.fx.project_id)
    .fetch_one(p.fx.handle.pool())
    .await?;
    p.fx.handle
        .lessons()
        .insert_lesson_relation(
            &crate::store::persistence::lesson_relations::LessonRelationDraft {
                from_lesson_id: other,
                to_lesson_id: p.old,
                kind: "contradicts",
                method: "adjudicated",
                status: "proposed",
                candidate_distance: None,
                evidence_json: json!({}),
                proposer_model: Some("a"),
                proposer_rationale: None,
                adjudicator_model: Some("b"),
                adjudicator_agrees: Some(true),
                adjudicator_rationale: None,
                detector_version: "test",
                decided_by: None,
            },
        )
        .await?;
    p.fx.handle
        .lessons()
        .approve_lesson_candidate(p.new, None, "judge")
        .await?;
    assert_eq!(
        p.fx.handle
            .lessons()
            .confirm_judge_only_write_time_relations()
            .await?,
        1
    );

    assert_eq!(
        p.fx.handle
            .lessons()
            .apply_confirmed_lesson_relations()
            .await?,
        1
    );
    let (stage, reason) = stage_of(&p.fx, p.old).await?;
    assert_eq!(
        (stage.as_str(), reason.as_deref()),
        ("archived", Some("superseded"))
    );
    assert_eq!(stage_of(&p.fx, p.new).await?.0, "daily");
    assert_eq!(p.fx.handle.lessons().count_lesson_evidence(p.new).await?, 2);
    assert_eq!(p.fx.handle.lessons().count_lesson_evidence(p.old).await?, 0);

    let inherited = relation_row(&p.fx, other, p.new).await?;
    assert_eq!(
        (inherited.0.as_str(), inherited.1.as_str()),
        ("contradicts", "proposed")
    );
    let kept = relation_row(&p.fx, other, p.old).await?;
    assert_eq!(
        kept.0, "contradicts",
        "the original relation stays on the archived lesson"
    );

    let evidence: serde_json::Value = sqlx::query_scalar(
        "SELECT evidence_json FROM memory.lesson_relations WHERE from_lesson_id = $1 AND to_lesson_id = $2",
    )
    .bind(p.new)
    .bind(p.old)
    .fetch_one(p.fx.handle.pool())
    .await?;
    assert_eq!(evidence["archived_lesson_id"], p.old);
    assert_eq!(evidence["moved_evidence"], 1);
    assert_eq!(evidence["inherited_injections"], 0);

    assert_eq!(
        p.fx.handle
            .lessons()
            .apply_confirmed_lesson_relations()
            .await?,
        0
    );
    p.fx.db.cleanup().await?;
    Ok(())
}
