use super::*;

struct Evaluation {
    id: i64,
    uuid: Uuid,
    steerer_id: i64,
    operator_turn_id: i64,
    status: String,
    intervention_uuid: Option<Uuid>,
    dispatched_at: DateTime<Utc>,
    resolved_at: Option<DateTime<Utc>>,
    has_verdict: bool,
}

pub(super) async fn evaluations(
    tx: &mut sqlx::PgConnection,
    page: &mut FlowPage,
    ids: &[i64],
) -> Result<(), PersistError> {
    let rows = sqlx::query_as!(
        Evaluation,
        r#"SELECT id, uuid, steerer_id, operator_turn_id, status, intervention_uuid,
                  dispatched_at, resolved_at, verdict_json IS NOT NULL AS "has_verdict!"
             FROM steering.evaluations WHERE id = ANY($1) ORDER BY id"#,
        ids,
    )
    .fetch_all(tx)
    .await?;
    for row in rows {
        let mut started = mike::activity(
            page,
            "steering.evaluations",
            row.id,
            None,
            "dispatched",
            row.dispatched_at,
        );
        started.lifecycle = FlowLifecycle::Started;
        started.metadata.source_uuid = Some(row.uuid);
        started.metadata.steerer_id = Some(row.steerer_id);
        started.metadata.turn_id = Some(row.operator_turn_id);
        if let Some(resolved_at) = row.resolved_at {
            let mut resolved = mike::activity(
                page,
                "steering.evaluations",
                row.id,
                None,
                "resolved",
                resolved_at,
            );
            resolved.metadata = started.metadata.clone();
            resolved.lifecycle = FlowLifecycle::Completed;
            resolved.outcome = Some(mike::outcome(&row.status));
            resolved.metadata.verdict = Some(row.status);
            resolved.metadata.has_verdict = Some(row.has_verdict);
            resolved.metadata.intervention_uuid = row.intervention_uuid;
            mike::push(page, started);
            mike::push(page, resolved);
        } else {
            mike::push(page, started);
        }
    }
    Ok(())
}

struct Judgment {
    id: i64,
    outcome: String,
    quality_score: Option<f64>,
    model: Option<String>,
    rubric_version: i32,
    judged_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
}

pub(super) async fn judgments(
    tx: &mut sqlx::PgConnection,
    page: &mut FlowPage,
    ids: &[i64],
) -> Result<(), PersistError> {
    let rows = sqlx::query_as!(
        Judgment,
        r#"SELECT v.id, o.code AS outcome, v.quality_score::double precision AS "quality_score?",
                  CASE WHEN octet_length(v.judge_model) <= 128 THEN v.judge_model END AS "model?",
                  v.rubric_version, v.judged_at, v.created_at
             FROM session_judges.verdicts v JOIN session_judges.outcomes o ON o.id = v.outcome_id
            WHERE v.id = ANY($1) AND v.mode = 'live' ORDER BY v.id"#,
        ids,
    )
    .fetch_all(tx)
    .await?;
    for row in rows {
        let mut activity = mike::activity(
            page,
            "session_judges.verdicts",
            row.id,
            None,
            "judged",
            row.judged_at,
        );
        activity.recorded_at = row.created_at;
        activity.time_semantics = FlowTimeSemantics::StoredOutput;
        activity.outcome = Some(mike::outcome(&row.outcome));
        activity.metadata.verdict = Some(row.outcome);
        activity.metadata.quality_score = row.quality_score;
        activity.metadata.rubric_version = u32::try_from(row.rubric_version).ok();
        activity.metadata.model = build::vocabulary(row.model);
        mike::push(page, activity);
    }
    Ok(())
}
