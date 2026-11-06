use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::launch::fleet::events::{EventKind, FleetEvent, NewFleetEvent};
use crate::store::persistence::{PersistError, PersistHandle};

struct EventRow {
    id: i64,
    run_id: String,
    task: Option<String>,
    session_uuid: Option<Uuid>,
    kind: String,
    payload: serde_json::Value,
    occurred_at: DateTime<Utc>,
}

fn into_event(row: EventRow) -> Result<FleetEvent, PersistError> {
    let kind = EventKind::parse(&row.kind).ok_or_else(|| {
        PersistError::DataInvariant(format!("unknown fleet event kind {}", row.kind))
    })?;
    Ok(FleetEvent {
        id: row.id,
        run_id: row.run_id,
        task: row.task,
        session_uuid: row.session_uuid,
        kind,
        payload: row.payload,
        occurred_at: row.occurred_at,
    })
}

impl PersistHandle {
    pub async fn append_fleet_event(&self, event: &NewFleetEvent) -> Result<i64, PersistError> {
        let kind = event.payload.kind()?;
        let body = event.payload.body()?;
        crate::metrics::time_db("launch_fleet_events_append", async move {
            let row = sqlx::query_file!(
                "src/store/persistence/fleet/events_sql/append.sql",
                event.run_id,
                event.task,
                event.session_uuid,
                kind.as_str(),
                body
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(row.id)
        })
        .await
    }

    pub async fn fleet_events_since(
        &self,
        run_id: &str,
        after_id: i64,
    ) -> Result<Vec<FleetEvent>, PersistError> {
        crate::metrics::time_db("launch_fleet_events_since", async move {
            let rows = sqlx::query_file!(
                "src/store/persistence/fleet/events_sql/since.sql",
                run_id,
                after_id
            )
            .fetch_all(&self.pool)
            .await?;
            rows.into_iter()
                .map(|r| {
                    into_event(EventRow {
                        id: r.id,
                        run_id: r.run_id,
                        task: r.task,
                        session_uuid: r.session_uuid,
                        kind: r.kind,
                        payload: r.payload,
                        occurred_at: r.occurred_at,
                    })
                })
                .collect()
        })
        .await
    }

    pub async fn fleet_events_for_task(
        &self,
        run_id: &str,
        task: &str,
    ) -> Result<Vec<FleetEvent>, PersistError> {
        crate::metrics::time_db("launch_fleet_events_for_task", async move {
            let rows = sqlx::query_file!(
                "src/store/persistence/fleet/events_sql/for_task.sql",
                run_id,
                task
            )
            .fetch_all(&self.pool)
            .await?;
            rows.into_iter()
                .map(|r| {
                    into_event(EventRow {
                        id: r.id,
                        run_id: r.run_id,
                        task: r.task,
                        session_uuid: r.session_uuid,
                        kind: r.kind,
                        payload: r.payload,
                        occurred_at: r.occurred_at,
                    })
                })
                .collect()
        })
        .await
    }
}
