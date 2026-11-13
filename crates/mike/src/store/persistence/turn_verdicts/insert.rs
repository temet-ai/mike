use super::super::PersistError;
use super::{LegacyStamp, TurnVerdictInsert, VerdictShape};

struct ShapeBinds<'a> {
    reaction: Option<&'a str>,
    delivery: Option<&'a str>,
    legacy_label: Option<&'a str>,
    failure_tags: Option<Vec<String>>,
    severity: Option<&'a str>,
}

fn shape_binds<'a>(shape: &VerdictShape<'a>) -> ShapeBinds<'a> {
    match shape {
        VerdictShape::TwoAxis {
            reaction,
            delivery,
            failure_tags,
            severity,
        } => ShapeBinds {
            reaction: Some(reaction),
            delivery: Some(delivery),
            legacy_label: None,
            failure_tags: failure_tags.clone(),
            severity: *severity,
        },
        VerdictShape::Legacy { label } => ShapeBinds {
            reaction: None,
            delivery: None,
            legacy_label: Some(label),
            failure_tags: None,
            severity: None,
        },
    }
}

async fn insert_row<'e, E>(
    executor: E,
    v: &TurnVerdictInsert<'_>,
    projected: bool,
) -> Result<Option<i64>, sqlx::Error>
where
    E: sqlx::PgExecutor<'e>,
{
    let shape = shape_binds(&v.judgement.shape);
    async move {
        sqlx::query_file_scalar!(
            "src/store/persistence/turn_verdicts/insert.sql",
            v.turn_id,
            v.mode.as_str(),
            v.bundle_id,
            v.rubric_version,
            v.input_builder_version,
            &v.slices as &[String],
            v.model,
            shape.reaction,
            shape.delivery,
            shape.legacy_label,
            v.judgement.confidence,
            v.judgement.rationale,
            v.judgement.unmet_ask,
            v.judgement.evidence_quality,
            v.next_genuine_request_id,
            v.as_of,
            v.input_text,
            v.usage.generation_id.as_deref(),
            v.usage.cost_usd,
            shape.failure_tags.as_deref() as Option<&[String]>,
            shape.severity,
            v.usage.prompt_tokens,
            v.usage.completion_tokens,
            v.usage.latency_ms,
            projected,
        )
        .fetch_optional(executor)
        .await
    }
    .await
}

async fn insert_live_row<'e, E>(
    executor: E,
    v: &TurnVerdictInsert<'_>,
    outcome_code: &str,
) -> Result<Option<(i64, bool)>, sqlx::Error>
where
    E: sqlx::PgExecutor<'e>,
{
    let shape = shape_binds(&v.judgement.shape);
    let row = async move {
        sqlx::query_file!(
            "src/store/persistence/turn_verdicts/insert_live.sql",
            v.turn_id,
            v.mode.as_str(),
            v.bundle_id,
            v.rubric_version,
            v.input_builder_version,
            &v.slices as &[String],
            v.model,
            shape.reaction,
            shape.delivery,
            shape.legacy_label,
            v.judgement.confidence,
            v.judgement.rationale,
            v.judgement.unmet_ask,
            v.judgement.evidence_quality,
            v.next_genuine_request_id,
            v.as_of,
            v.input_text,
            v.usage.generation_id.as_deref(),
            v.usage.cost_usd,
            shape.failure_tags.as_deref() as Option<&[String]>,
            shape.severity,
            v.usage.prompt_tokens,
            v.usage.completion_tokens,
            v.usage.latency_ms,
            outcome_code,
        )
        .fetch_optional(executor)
        .await
    }
    .await?;
    Ok(row.map(|row| (row.id, row.projected)))
}

impl crate::store::persistence::eval_repo::EvalDb<'_> {
    pub async fn insert_turn_verdict(self, v: TurnVerdictInsert<'_>) -> Result<i64, PersistError> {
        crate::metrics::time_db("insert_turn_verdict", async move {
            let id = insert_row(&self.pool, &v, false).await?;
            id.ok_or(PersistError::TurnNotFound(v.turn_id))
        })
        .await
    }

    pub async fn insert_live_legacy_verdict_and_stamp(
        self,
        v: TurnVerdictInsert<'_>,
        stamp: LegacyStamp<'_>,
    ) -> Result<(i64, bool), PersistError> {
        crate::metrics::time_db("insert_live_legacy_verdict_and_stamp", async move {
            let row = insert_live_row(&self.pool, &v, stamp.outcome_code).await?;
            row.ok_or(PersistError::TurnNotFound(v.turn_id))
        })
        .await
    }
}

pub(crate) async fn insert_turn_verdict_on(
    conn: &mut sqlx::PgConnection,
    v: &TurnVerdictInsert<'_>,
    projected: bool,
) -> Result<i64, PersistError> {
    let id = insert_row(&mut *conn, v, projected).await?;
    id.ok_or(PersistError::TurnNotFound(v.turn_id))
}
