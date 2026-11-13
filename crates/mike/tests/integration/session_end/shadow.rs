use chrono::Utc;
use mike::store::persistence::eval_repo::EvalPersist as _;
use mike::store::persistence::operator_turns::projection::{ProjectionMode, ProjectionParams};
use mike::store::persistence::session_end::SessionVerdictV3Row;

use crate::judge_quality::promotion_support::{closed_turn, facts, projected};
use crate::turn_facts::{Fixture, fixture};

async fn candidate_session_bundle(f: &Fixture) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "INSERT INTO eval.bundles (role, subject_key, origin, status, components, content_sha256, build_sha)
         VALUES ('session_judge', '', 'candidate', 'candidate', '{}'::jsonb, 'sha-' || gen_random_uuid(), 'build')
         RETURNING id",
    )
    .fetch_one(f.handle.pool())
    .await?)
}

fn shadow_row(
    turn: i64,
    session_id: i64,
    bundle_id: i64,
    outcome_id: i64,
) -> SessionVerdictV3Row<'static> {
    SessionVerdictV3Row {
        session_db_id: session_id,
        outcome_id,
        quality_score: 0.2,
        rationale: "shadow candidate: the operator gave up",
        judge_model: "google/gemini-3.5-flash",
        rubric_version: 3,
        mode: "shadow",
        bundle_id: Some(bundle_id),
        rubric_id: "session-v3-2026-09-24",
        end_turn_id: Some(turn),
        end_label: Some("closed_in_frustration"),
        end_source: Some("llm"),
        end_rule: None,
        end_confidence: Some("high"),
        end_evidence: Some("operator: forget it, I'll do it myself"),
        failure: Some("the retry was never tested"),
        failure_tags: vec!["unverified-claim".to_owned()],
        severity: Some("major"),
        slices: Vec::new(),
        as_of: Utc::now(),
        digest_version: "dossier-r1",
        input_text: "SESSION DIGEST",
        llm_output: serde_json::json!({"end_label": "closed_in_frustration"}),
        generation_id: None,
        cost_usd: Some(0.001),
        prompt_tokens: Some(1_000),
        completion_tokens: Some(120),
        latency_ms: Some(400),
    }
}

#[tokio::test]
async fn a_shadow_verdict_is_never_read_by_projection() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let ending = closed_turn(&f, 33_001).await?;
    facts(&f, ending, "genuine", "none_session_settled").await?;

    let session_bundle = candidate_session_bundle(&f).await?;
    let outcome_id: i64 =
        sqlx::query_scalar("SELECT id FROM session_judges.outcomes WHERE code = 'failure'")
            .fetch_one(f.handle.pool())
            .await?;

    let before_insert = projected(&f, ending).await?;
    let verdict_id = f
        .handle
        .eval()
        .insert_session_verdict_v3(&shadow_row(
            ending,
            f.session_id,
            session_bundle,
            outcome_id,
        ))
        .await?;
    let after_insert = projected(&f, ending).await?;
    assert_eq!(
        after_insert, before_insert,
        "writing a session_judges.verdicts row with mode = 'shadow' must not touch operator_turns"
    );

    let recorded_mode: String =
        sqlx::query_scalar("SELECT mode FROM session_judges.verdicts WHERE id = $1")
            .bind(verdict_id)
            .fetch_one(f.handle.pool())
            .await?;
    assert_eq!(recorded_mode, "shadow");

    f.handle
        .project_turn_outcomes(
            &[ending],
            &ProjectionParams {
                live_bundle_id: 0,
                confidence_threshold: 0.5,
                mode: ProjectionMode::Apply,
                session_judge_live_bundle_id: session_bundle,
            },
        )
        .await?;
    let after_projection = projected(&f, ending).await?;
    assert_ne!(
        after_projection.outcome, "abandoned",
        "the frustration close in the shadow row must not reach operator_turns.outcome_id"
    );
    assert_ne!(
        after_projection.outcome_source.as_deref(),
        Some("frustration_close")
    );
    Ok(())
}
