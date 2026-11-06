use super::*;

struct StageRow {
    id: i64,
    created_at: DateTime<Utc>,
    ordinal: Option<i64>,
    stage: Option<String>,
    outcome: Option<String>,
    valid: bool,
    firing_rule_count: Option<i32>,
}

pub(super) async fn manifests(
    tx: &mut sqlx::PgConnection,
    page: &mut FlowPage,
    ids: &[i64],
) -> Result<(), PersistError> {
    let rows = sqlx::query_file_as!(
        StageRow,
        "src/store/persistence/session_flow/manifest_stages.sql",
        ids,
    )
    .fetch_all(tx)
    .await?;
    for row in rows {
        let stage = build::vocabulary(row.stage);
        if !row.valid || (row.ordinal.is_some() && (stage.is_none() || row.outcome.is_none())) {
            page.coverage.stale_reason = Some("manifest_missing_malformed_or_unsupported: at most 64 stages supported; whole source omitted".into());
            page.coverage.retention = FlowRetention::Partial;
            continue;
        }
        let Some(ordinal) = row.ordinal else {
            continue;
        };
        let mut activity = mike::activity(
            page,
            "requests.requests",
            row.id,
            Some(row.id),
            &format!("stage:{ordinal}"),
            row.created_at,
        );
        activity.metadata.ordinal = u32::try_from(ordinal).ok();
        activity.metadata.schema_version = Some(1);
        activity.metadata.stage = stage;
        activity.metadata.firing_rule_count =
            row.firing_rule_count.and_then(|v| u32::try_from(v).ok());
        activity.outcome = row.outcome.as_deref().map(mike::outcome);
        mike::push(page, activity);
    }
    Ok(())
}

struct InjectionRow {
    id: i64,
    lesson_id: i64,
    request_id: i64,
    injected_at: DateTime<Utc>,
    lane: String,
    persona: Option<String>,
}

pub(super) async fn injections(
    tx: &mut sqlx::PgConnection,
    page: &mut FlowPage,
    ids: &[i64],
) -> Result<(), PersistError> {
    let rows = sqlx::query_as!(
        InjectionRow,
        r#"SELECT i.id, i.lesson_id, i.request_id, i.injected_at, l.code AS lane,
                  CASE WHEN octet_length(i.persona) <= 128 THEN i.persona END AS "persona?"
             FROM memory.lesson_injections i JOIN memory.lesson_injection_lanes l ON l.id = i.lane_id
            WHERE i.id = ANY($1) ORDER BY i.id"#,
        ids,
    )
    .fetch_all(tx)
    .await?;
    for row in rows {
        let mut activity = mike::activity(
            page,
            "memory.lesson_injections",
            row.id,
            Some(row.request_id),
            "injected",
            row.injected_at,
        );
        activity.metadata.lesson_id = Some(row.lesson_id);
        activity.metadata.role = build::vocabulary(Some(row.lane));
        activity.metadata.persona = build::vocabulary(row.persona);
        mike::push(page, activity);
    }
    Ok(())
}
