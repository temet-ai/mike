use uuid::Uuid;

use super::fleet_target::seat_sessions;
use crate::launch::fleet::FleetDefinition;
use crate::launch::fleet::definition::{recorded_task_of, with_task_seats};
use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClosedSeat {
    pub(crate) task: Option<String>,
    pub(crate) successor: Option<Uuid>,
    pub(crate) coordinator: Option<Uuid>,
}

impl ClosedSeat {
    pub(crate) fn refusal(&self, closed: Uuid) -> String {
        let task = self
            .task
            .as_deref()
            .map(|t| format!(" for task {t}"))
            .unwrap_or_default();
        match (self.successor, self.coordinator) {
            (Some(next), _) => format!(
                "session {closed} is closed and will never read this. The seat that now has its task is session {next}: send it there"
            ),
            (None, Some(lead)) => format!(
                "session {closed} is closed and will never read this, and no open seat has its work{task}. The coordinator is session {lead}: send it there"
            ),
            (None, None) => format!(
                "session {closed} is closed and will never read this, and the fleet has no open coordinator{task}: tell the operator in a note with status operator"
            ),
        }
    }
}

struct LaunchSeat {
    run_id: Option<String>,
    seat: Option<String>,
    closed: bool,
}

pub(crate) fn task_of_session(
    definition: &FleetDefinition,
    session: Uuid,
    seat: Option<&str>,
) -> Option<String> {
    let wanted = session.to_string();
    definition
        .tasks
        .iter()
        .find(|(_, t)| {
            t.session.as_deref() == Some(wanted.as_str())
                || seat.is_some_and(|s| t.seat.as_deref() == Some(s))
        })
        .map(|(id, _)| id.clone())
}

fn parsed(session: Option<&str>) -> Option<Uuid> {
    session.and_then(|s| Uuid::parse_str(s).ok())
}

impl PersistHandle {
    async fn latest_launch_seat(&self, session: Uuid) -> Result<Option<LaunchSeat>, PersistError> {
        crate::metrics::time_db("mailbox_latest_launch_seat", async move {
            let row = sqlx::query!(
                r#"SELECT l.run_id, l.seat,
                          (l.closed_at IS NOT NULL
                           AND (s.last_seen IS NULL
                                OR s.last_seen <= l.closed_at + interval '5 seconds')) AS "closed!"
                     FROM launch.launches l
                     LEFT JOIN sessions.sessions s ON s.session_uuid = l.session_uuid
                    WHERE l.session_uuid = $1
                    ORDER BY l.created_at DESC, l.id DESC
                    LIMIT 1"#,
                session,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row.map(|r| LaunchSeat {
                run_id: r.run_id,
                seat: r.seat,
                closed: r.closed,
            }))
        })
        .await
    }

    pub(crate) async fn closed_fleet_seat(
        &self,
        to: Uuid,
    ) -> Result<Option<ClosedSeat>, PersistError> {
        if !crate::launch::seat_fact::is_fleet_seat(self, to).await {
            return Ok(None);
        }
        let Some(launch) = self.latest_launch_seat(to).await? else {
            return Ok(None);
        };
        let (true, Some(run)) = (launch.closed, launch.run_id.as_deref()) else {
            return Ok(None);
        };
        let unknown = ClosedSeat {
            task: None,
            successor: None,
            coordinator: None,
        };
        let Some(row) = self.find_fleet_for_session(to).await? else {
            return Ok(Some(unknown));
        };
        if row.run_id != launch.run_id {
            return Ok(Some(unknown));
        }
        let Ok(definition) = FleetDefinition::from_value(&row.definition) else {
            return Ok(Some(unknown));
        };
        let events = self.fleet_events_since(run, 0).await?;
        let definition = with_task_seats(definition, &events);
        let task = task_of_session(&definition, to, launch.seat.as_deref())
            .or_else(|| recorded_task_of(&events, to));
        let task_session = task
            .as_deref()
            .and_then(|id| definition.tasks.get(id))
            .and_then(|t| parsed(t.session.as_deref()))
            .filter(|s| *s != to);
        let coordinator = parsed(definition.coordinator.session.as_deref()).filter(|s| *s != to);
        let pool: Vec<Uuid> = task_session.into_iter().chain(coordinator).collect();
        let open = self.open_sessions_among(&pool).await?;
        Ok(Some(ClosedSeat {
            task,
            successor: task_session.filter(|s| open.contains(s)),
            coordinator: coordinator.filter(|s| open.contains(s)),
        }))
    }

    pub(crate) async fn sender_label(&self, from: Uuid) -> Option<String> {
        match self.sender_label_inner(from).await {
            Ok(label) => label,
            Err(error) => {
                tracing::warn!(%error, %from, "the sender's seat could not be looked up for the mail envelope");
                None
            }
        }
    }

    async fn sender_label_inner(&self, from: Uuid) -> Result<Option<String>, PersistError> {
        if let Some(definition) = self.sender_fleet_definition(from).await?
            && seat_sessions(&definition).contains(&from)
        {
            let launch = self.latest_launch_seat(from).await?;
            let seat = launch.as_ref().and_then(|l| l.seat.as_deref());
            let task = task_of_session(&definition, from, None);
            return Ok(Some(fleet_label(seat, task.as_deref(), from)));
        }
        let name = self.session_display_name(from).await?;
        Ok(name.map(|n| format!("{n} (session {from})")))
    }

    async fn session_display_name(&self, session: Uuid) -> Result<Option<String>, PersistError> {
        crate::metrics::time_db("mailbox_session_display_name", async move {
            let name = sqlx::query_scalar!(
                r#"SELECT sg.display_name AS "display_name?"
                     FROM sessions.session_goals sg
                     JOIN sessions.sessions s ON s.id = sg.session_id
                    WHERE s.session_uuid = $1"#,
                session,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(name.flatten().filter(|n| !n.trim().is_empty()))
        })
        .await
    }
}

pub(crate) fn fleet_label(seat: Option<&str>, task: Option<&str>, session: Uuid) -> String {
    let seat = seat.unwrap_or("fleet seat");
    match task {
        Some(task) => format!("{seat} (task {task}, session {session})"),
        None => format!("{seat} (session {session})"),
    }
}
