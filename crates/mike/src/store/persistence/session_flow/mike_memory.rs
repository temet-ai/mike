use super::*;
use std::collections::BTreeSet;

struct Embedding {
    embedding_model: Option<String>,
    source: String,
    embedded_at: DateTime<Utc>,
}

pub(super) async fn embedding(
    tx: &mut sqlx::PgConnection,
    page: &mut FlowPage,
) -> Result<(), PersistError> {
    page.coverage.latest_only = true;
    let row = sqlx::query_as!(
        Embedding,
        r#"SELECT CASE WHEN octet_length(embedding_model) <= 128 THEN embedding_model END AS "embedding_model?",
                  source, embedded_at
             FROM memory.session_embeddings WHERE session_id = $1"#,
        page.session_id,
    )
    .fetch_optional(tx)
    .await?;
    let Some(row) = row else {
        return Ok(());
    };
    let source = FlowSource {
        table: "memory.session_embeddings".into(),
        row_id: FlowRowId::Uuid(page.session_id),
        session_id: Some(page.session_id),
        request_id: None,
        detail_href: None,
    };
    let mut activity = build::activity(
        source,
        "embedded",
        format!("mike:{}:embedding", page.session_id),
        FlowComponent::Embeddings,
        row.embedded_at,
    );
    activity.time_semantics = FlowTimeSemantics::StoredOutput;
    activity.metadata.model = build::vocabulary(row.embedding_model);
    activity.metadata.scope = Some(row.source);
    page.coverage.lower_id = Some(FlowRowId::Uuid(page.session_id));
    page.coverage.upper_id = page.coverage.lower_id.clone();
    page.coverage.returned_through_id = page.coverage.lower_id.clone();
    mike::push(page, activity);
    Ok(())
}

struct Action {
    id: i64,
    run_id: i64,
    lesson_id: Option<i64>,
    lesson_uuid: Uuid,
    source_turn_id: Option<i64>,
    action: String,
    outcome: String,
    created_at: DateTime<Utc>,
}

pub(super) async fn consolidation(
    tx: &mut sqlx::PgConnection,
    page: &mut FlowPage,
    ids: &[i64],
) -> Result<(), PersistError> {
    let rows = sqlx::query_as!(
        Action,
        r#"SELECT a.id, a.run_id, a.lesson_id, a.lesson_uuid, a.source_turn_id,
                  k.code AS action, o.code AS outcome, a.created_at
             FROM memory.consolidation_actions a
             JOIN memory.consolidation_action_kinds k ON k.id = a.action_kind_id
             JOIN memory.consolidation_outcomes o ON o.id = a.outcome_id
            WHERE a.id = ANY($1) ORDER BY a.id"#,
        ids,
    )
    .fetch_all(&mut *tx)
    .await?;
    let mut runs = BTreeSet::new();
    for row in rows {
        runs.insert(row.run_id);
        let mut activity = mike::activity(
            page,
            "memory.consolidation_actions",
            row.id,
            None,
            "recorded",
            row.created_at,
        );
        activity.metadata.run_id = Some(row.run_id);
        activity.metadata.lesson_id = row.lesson_id;
        activity.metadata.lesson_uuid = Some(row.lesson_uuid);
        activity.metadata.turn_id = row.source_turn_id;
        activity.metadata.stage = Some(row.action);
        activity.metadata.verdict = Some(row.outcome.clone());
        activity.outcome = Some(mike::outcome(&row.outcome));
        mike::push(page, activity);
    }
    shared_runs(tx, page, &runs.into_iter().collect::<Vec<_>>()).await
}

struct Run {
    id: i64,
    uuid: Uuid,
    kind: String,
    status: String,
    started_at: DateTime<Utc>,
    finished_at: Option<DateTime<Utc>>,
}

async fn shared_runs(
    tx: &mut sqlx::PgConnection,
    page: &mut FlowPage,
    ids: &[i64],
) -> Result<(), PersistError> {
    let rows = sqlx::query_as!(
        Run,
        r#"SELECT r.id, r.uuid, k.code AS kind, s.code AS status, r.started_at, r.finished_at
             FROM memory.consolidation_runs r
             JOIN memory.consolidation_run_kinds k ON k.id = r.run_kind_id
             JOIN memory.consolidation_run_statuses s ON s.id = r.status_id
            WHERE r.id = ANY($1) ORDER BY r.id"#,
        ids,
    )
    .fetch_all(tx)
    .await?;
    for row in rows {
        for (phase, time) in [
            ("started", Some(row.started_at)),
            ("finished", row.finished_at),
        ] {
            let Some(time) = time else {
                continue;
            };
            let mut activity =
                mike::activity(page, "memory.consolidation_runs", row.id, None, phase, time);
            activity.source.session_id = None;
            activity.actor_id = format!("mike:shared-run:{}", row.id);
            activity.metadata.source_uuid = Some(row.uuid);
            activity.metadata.run_id = Some(row.id);
            activity.metadata.scope = Some("shared".into());
            activity.metadata.role = Some(row.kind.clone());
            activity.lifecycle = if phase == "started" {
                FlowLifecycle::Started
            } else {
                FlowLifecycle::Completed
            };
            if phase == "finished" {
                activity.metadata.verdict = Some(row.status.clone());
                activity.outcome = Some(mike::outcome(&row.status));
            }
            let mut actor = build::actor(
                page.session_id,
                FlowActorKind::Mike,
                activity.actor_id.clone(),
                "Shared consolidation run".into(),
                Some(activity.source.clone()),
            );
            actor.session_id = None;
            page.actors.push(actor);
            page.activities.push(activity);
        }
    }
    Ok(())
}
