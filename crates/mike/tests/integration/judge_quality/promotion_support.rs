use chrono::Utc;
use mike::store::persistence::operator_turns::projection::{ProjectionMode, ProjectionParams};
use mike::store::persistence::turn_verdicts::{
    TurnVerdictInsert, VerdictCallUsage, VerdictJudgement, VerdictMode, VerdictShape,
};

use crate::turn_facts::{Fixture, NewTurn, Req, insert_request, insert_turn, sample_row};

pub(crate) async fn bundle(f: &Fixture) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "INSERT INTO eval.bundles (role, subject_key, origin, status, components, content_sha256, build_sha, promoted_at)
         VALUES ('turn_judge', '', 'candidate', 'candidate', '{}'::jsonb, 'sha-' || gen_random_uuid(), 'build', now() - interval '1 day')
         RETURNING id",
    )
    .fetch_one(f.handle.pool())
    .await?)
}

pub(crate) async fn closed_turn(f: &Fixture, start: i64) -> anyhow::Result<i64> {
    insert_request(f, &Req::primary(start, 0, "add the retry and test it")).await?;
    insert_turn(f, &NewTurn::closed(start, start, 0, 30)).await
}

pub(crate) async fn facts(
    f: &Fixture,
    turn: i64,
    prompt_kind: &str,
    next_state: &str,
) -> anyhow::Result<()> {
    let mut row = sample_row(turn, f.session_id, Utc::now());
    row.prompt_kind = prompt_kind.to_owned();
    row.next_state = next_state.to_owned();
    if next_state == "resolved" {
        crate::turn_facts::resolve(&mut row);
    }
    f.handle.upsert_turn_facts(&[row]).await?;
    Ok(())
}

pub(crate) fn verdict(
    turn_id: i64,
    mode: VerdictMode,
    bundle_id: i64,
    axes: (&'static str, &'static str),
    confidence: f32,
) -> TurnVerdictInsert<'static> {
    TurnVerdictInsert {
        turn_id,
        mode,
        bundle_id: Some(bundle_id),
        rubric_version: "v2-2026-09-23",
        input_builder_version: "judge-input-v2.1",
        slices: Vec::new(),
        model: "google/gemini-3.5-flash",
        judgement: VerdictJudgement {
            shape: VerdictShape::TwoAxis {
                reaction: axes.0,
                delivery: axes.1,
                failure_tags: None,
                severity: None,
            },
            confidence,
            rationale: "the operator's next message says the retry was never tested",
            unmet_ask: "run the tests",
            evidence_quality: "asserted",
        },
        next_genuine_request_id: None,
        as_of: Utc::now(),
        input_text: "OPERATOR REQUEST",
        usage: VerdictCallUsage::default(),
    }
}

pub(crate) fn params(bundle_id: i64, mode: ProjectionMode) -> ProjectionParams {
    ProjectionParams {
        live_bundle_id: bundle_id,
        confidence_threshold: 0.5,
        mode,
        session_judge_live_bundle_id: 0,
    }
}

#[derive(Debug, PartialEq, Eq, sqlx::FromRow)]
pub(crate) struct Projected {
    pub(crate) outcome: String,
    pub(crate) outcome_source: Option<String>,
    pub(crate) reaction: Option<String>,
    pub(crate) delivery: Option<String>,
    pub(crate) live_verdict_id: Option<i64>,
    pub(crate) outcome_bundle_id: Option<i64>,
    pub(crate) outcome_projection_version: Option<i16>,
}

pub(crate) async fn projected(f: &Fixture, turn: i64) -> anyhow::Result<Projected> {
    Ok(sqlx::query_as(
        "SELECT o.code AS outcome, t.outcome_source, t.reaction, t.delivery, t.live_verdict_id,
                t.outcome_bundle_id, t.outcome_projection_version
           FROM memory.operator_turns t JOIN memory.turn_outcomes o ON o.id = t.outcome_id
          WHERE t.id = $1",
    )
    .bind(turn)
    .fetch_one(f.handle.pool())
    .await?)
}

pub(crate) async fn set_legacy(f: &Fixture, turn: i64, code: &str) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE memory.operator_turns t SET outcome_id = o.id, judge_model = 'r3-model', rationale = 'legacy'
           FROM memory.turn_outcomes o WHERE o.code = $2 AND t.id = $1",
    )
    .bind(turn)
    .bind(code)
    .execute(f.handle.pool())
    .await?;
    Ok(())
}

pub(crate) async fn gold(f: &Fixture, turn: i64, axis: &str, label: &str) -> anyhow::Result<i64> {
    crate::turn_facts::dev_split(f, turn).await?;
    Ok(sqlx::query_scalar(
        "INSERT INTO eval.labels (subject_kind, subject_id, operator_turn_id, session_id, session_uuid,
                                  axis, label, tier, method, split, source)
         SELECT 'turn', t.id, t.id, t.session_id, s.session_uuid, $2, $3, 'gold', 'operator_click', 'dev', 'test'
           FROM memory.operator_turns t JOIN sessions.sessions s ON s.id = t.session_id
          WHERE t.id = $1
         RETURNING id",
    )
    .bind(turn)
    .bind(axis)
    .bind(label)
    .fetch_one(f.handle.pool())
    .await?)
}

pub(crate) fn resolved_verdict(
    turn_id: i64,
    mode: VerdictMode,
    bundle_id: i64,
    axes: (&'static str, &'static str),
    confidence: f32,
) -> TurnVerdictInsert<'static> {
    TurnVerdictInsert {
        next_genuine_request_id: Some(2),
        ..verdict(turn_id, mode, bundle_id, axes, confidence)
    }
}
