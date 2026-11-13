//! The rest of write_time_tests's tests.

use super::*;

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
