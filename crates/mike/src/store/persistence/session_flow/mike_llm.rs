use super::*;

struct Row {
    id: i64,
    created_at: DateTime<Utc>,
    role: String,
    model: Option<String>,
    steerer_id: Option<i64>,
    operator_turn_id: Option<i64>,
    outcome: String,
    prompt_tokens: Option<i32>,
    completion_tokens: Option<i32>,
    cost_usd: Option<f64>,
    latency_ms: Option<i32>,
    http_status: Option<i32>,
}

pub(super) async fn read(
    tx: &mut sqlx::PgConnection,
    page: &mut FlowPage,
    ids: &[i64],
) -> Result<(), PersistError> {
    let rows = sqlx::query_as!(
        Row,
        r#"SELECT id, created_at, role, CASE WHEN octet_length(model) <= 128 THEN model END AS "model?",
                  steerer_id, operator_turn_id, outcome, prompt_tokens, completion_tokens,
                  cost_usd, latency_ms, http_status
             FROM llm.calls WHERE id = ANY($1) ORDER BY id"#,
        ids,
    )
    .fetch_all(tx)
    .await?;
    for row in rows {
        let mut activity =
            mike::activity(page, "llm.calls", row.id, None, "recorded", row.created_at);
        activity.occurred_at = None;
        let refused = row.outcome.starts_with("refused_");
        activity.time_semantics = if !refused && row.latency_ms.is_some() {
            FlowTimeSemantics::MeasuredLatency
        } else {
            FlowTimeSemantics::StoredOutput
        };
        activity.outcome = Some(mike::outcome(&row.outcome));
        activity.metadata.verdict = Some(row.outcome);
        activity.metadata.role = build::vocabulary(Some(row.role));
        activity.metadata.model = build::vocabulary(row.model);
        activity.metadata.steerer_id = row.steerer_id;
        activity.metadata.turn_id = row.operator_turn_id;
        activity.metadata.input_tokens = row.prompt_tokens.map(i64::from);
        activity.metadata.output_tokens = row.completion_tokens.map(i64::from);
        activity.metadata.cost_usd = row.cost_usd.filter(|v| v.is_finite());
        activity.metadata.latency_ms = if refused {
            None
        } else {
            row.latency_ms.filter(|v| *v >= 0).map(f64::from)
        };
        activity.metadata.status_code = row.http_status.and_then(|v| u16::try_from(v).ok());
        mike::push(page, activity);
    }
    Ok(())
}
