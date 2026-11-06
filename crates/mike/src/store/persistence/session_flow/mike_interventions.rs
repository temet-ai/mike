use super::*;

struct Row {
    id: i64,
    uuid: Uuid,
    steerer_id: i64,
    operator_turn_id: Option<i64>,
    verdict: String,
    trigger_request_id: Option<i64>,
    sent_request_id: Option<i64>,
    delivered_request_id: Option<i64>,
    created_at: DateTime<Utc>,
    sent_at: Option<DateTime<Utc>>,
    processed_at: Option<DateTime<Utc>>,
    expires_at: Option<DateTime<Utc>>,
    reverted_at: Option<DateTime<Utc>>,
}

pub(super) async fn read(
    tx: &mut sqlx::PgConnection,
    page: &mut FlowPage,
    ids: &[i64],
) -> Result<(), PersistError> {
    let rows = sqlx::query_as!(
        Row,
        r#"SELECT i.id, i.uuid, i.steerer_id, i.operator_turn_id, v.code AS verdict,
                  i.trigger_request_id, i.sent_request_id, i.delivered_request_id,
                  i.created_at, i.sent_at, i.processed_at, i.expires_at, i.reverted_at
             FROM steering.interventions i JOIN steering.verdicts v ON v.id = i.verdict_id
            WHERE i.id = ANY($1) ORDER BY i.id"#,
        ids,
    )
    .fetch_all(tx)
    .await?;
    for row in rows {
        append(page, row);
    }
    Ok(())
}

fn append(page: &mut FlowPage, row: Row) {
    let phases = [
        (
            if row.verdict == "fired" {
                "fired"
            } else {
                "observed"
            },
            Some(row.created_at),
            row.trigger_request_id,
            FlowLifecycle::Observed,
        ),
        (
            "attempted",
            row.sent_at,
            row.sent_request_id,
            FlowLifecycle::Attempted,
        ),
        (
            "delivered",
            row.processed_at,
            row.delivered_request_id,
            FlowLifecycle::Delivered,
        ),
        ("reverted", row.reverted_at, None, FlowLifecycle::Reverted),
    ];
    for (phase, time, request, lifecycle) in phases {
        let Some(time) = time else {
            continue;
        };
        let mut activity =
            mike::activity(page, "steering.interventions", row.id, request, phase, time);
        activity.lifecycle = lifecycle;
        activity.metadata.source_uuid = Some(row.uuid);
        activity.metadata.steerer_id = Some(row.steerer_id);
        activity.metadata.turn_id = row.operator_turn_id;
        if matches!(phase, "fired" | "observed") {
            activity.metadata.verdict = Some(row.verdict.clone());
            activity.metadata.expires_at = row.expires_at;
            if row.sent_at.is_none()
                && row.reverted_at.is_none()
                && row
                    .expires_at
                    .is_some_and(|expires_at| expires_at < Utc::now())
            {
                activity.outcome = Some(FlowOutcome::Expired);
            }
        }
        if phase == "reverted" {
            activity.outcome = Some(FlowOutcome::Reverted);
        }
        mike::push(page, activity);
    }
}
