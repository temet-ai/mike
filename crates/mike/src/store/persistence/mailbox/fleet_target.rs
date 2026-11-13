use crate::store::persistence::fleet_repo::FleetPersist as _;
use std::collections::BTreeSet;

use uuid::Uuid;

use crate::launch::fleet::FleetDefinition;
use crate::launch::fleet::checks::normal;
use crate::launch::fleet::commit::covers;
use crate::launch::fleet::definition::with_task_seats;
use crate::store::persistence::PersistError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FleetTarget {
    Seat { session: Uuid, task: Option<String> },
    Seats(Vec<Uuid>),
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum TargetError {
    #[error(
        "`{0}` addresses a seat of the sender's fleet, and the sender is not a seat of a running fleet; address a session by id or prefix, `name:<name>` or `role:<role>` instead"
    )]
    NotInFleet(String),
    #[error("`{0}` needs something after the colon")]
    Empty(&'static str),
    #[error("the fleet has no task `{given}`; its tasks are {known}")]
    UnknownTask { given: String, known: String },
    #[error("{0} has no seat yet: nothing has been started for it")]
    NoSeatYet(String),
    #[error("no other seat of the fleet is open")]
    NoOpenSeats,
    #[error(transparent)]
    Persist(#[from] PersistError),
}

enum TargetForm {
    Task(String),
    Owner(String),
    Fleet,
}

impl TargetForm {
    fn parse(to: &str) -> Result<Option<Self>, TargetError> {
        let to = to.trim();
        if to == "fleet" {
            return Ok(Some(Self::Fleet));
        }
        if let Some(id) = to.strip_prefix("task:") {
            return non_empty(id, "task:").map(|id| Some(Self::Task(id)));
        }
        if let Some(path) = to.strip_prefix("owner:") {
            return non_empty(path, "owner:").map(|path| Some(Self::Owner(path)));
        }
        Ok(None)
    }
}

fn non_empty(text: &str, form: &'static str) -> Result<String, TargetError> {
    let text = text.trim();
    if text.is_empty() {
        return Err(TargetError::Empty(form));
    }
    Ok(text.to_owned())
}

fn session_of(text: Option<&str>) -> Option<Uuid> {
    text.and_then(|s| Uuid::parse_str(s).ok())
}

pub(crate) fn seat_of_task(
    definition: &FleetDefinition,
    task_id: &str,
) -> Result<FleetTarget, TargetError> {
    let Some(task) = definition.tasks.get(task_id) else {
        return Err(TargetError::UnknownTask {
            given: task_id.to_owned(),
            known: definition
                .tasks
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", "),
        });
    };
    session_of(task.session.as_deref())
        .map(|session| FleetTarget::Seat {
            session,
            task: Some(task_id.to_owned()),
        })
        .ok_or_else(|| TargetError::NoSeatYet(format!("task {task_id}")))
}

pub(crate) fn coordinator_seat(definition: &FleetDefinition) -> Result<FleetTarget, TargetError> {
    session_of(definition.coordinator.session.as_deref())
        .map(|session| FleetTarget::Seat {
            session,
            task: None,
        })
        .ok_or_else(|| TargetError::NoSeatYet("the coordinator".to_owned()))
}

pub(crate) fn seat_of_owner(
    definition: &FleetDefinition,
    path: &str,
) -> Result<FleetTarget, TargetError> {
    let wanted = normal(path);
    let owner = definition.tasks.iter().find(|(_, task)| {
        let owned: Vec<String> = task
            .files
            .iter()
            .chain(task.new_files.iter())
            .map(|f| normal(f))
            .collect();
        covers(&owned, &wanted)
    });
    match owner {
        Some((id, _)) => seat_of_task(definition, id),
        None => coordinator_seat(definition),
    }
}

pub(crate) fn seat_sessions(definition: &FleetDefinition) -> Vec<Uuid> {
    let tasks = definition.tasks.values().map(|t| t.session.as_deref());
    let others = [
        definition.coordinator.session.as_deref(),
        definition.acceptance.session.as_deref(),
        definition.observer.session.as_deref(),
        definition.reference.session.as_deref(),
    ];
    let mut sessions: Vec<Uuid> = tasks.chain(others).filter_map(session_of).collect();
    sessions.sort();
    sessions.dedup();
    sessions
}

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub(crate) async fn sender_fleet_definition(
        self,
        sender: Uuid,
    ) -> Result<Option<FleetDefinition>, PersistError> {
        if !crate::launch::seat_fact::is_fleet_seat(&self, sender).await {
            return Ok(None);
        }
        let Some(row) = self.fleet().find_fleet_for_session(sender).await? else {
            return Ok(None);
        };
        let Some(run) = row.run_id.as_deref() else {
            return Ok(None);
        };
        let Ok(definition) = FleetDefinition::from_value(&row.definition) else {
            return Ok(None);
        };
        let events = self.fleet().fleet_events_since(run, 0).await?;
        Ok(Some(with_task_seats(definition, &events)))
    }

    pub(crate) async fn open_sessions_among(
        self,
        sessions: &[Uuid],
    ) -> Result<BTreeSet<Uuid>, PersistError> {
        crate::metrics::time_db("mailbox_open_sessions_among", async move {
            let rows = sqlx::query_scalar!(
                r#"SELECT DISTINCT l.session_uuid AS "session_uuid!"
                     FROM launch.launches l
                     LEFT JOIN sessions.sessions s ON s.session_uuid = l.session_uuid
                    WHERE l.session_uuid = ANY($1)
                      AND (l.closed_at IS NULL
                           OR s.last_seen > l.closed_at + interval '5 seconds')"#,
                sessions,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows.into_iter().collect())
        })
        .await
    }

    pub(crate) async fn resolve_fleet_target(
        self,
        sender: Uuid,
        to: &str,
    ) -> Result<Option<FleetTarget>, TargetError> {
        let Some(form) = TargetForm::parse(to)? else {
            return Ok(None);
        };
        let Some(definition) = self.sender_fleet_definition(sender).await? else {
            return Err(TargetError::NotInFleet(to.trim().to_owned()));
        };
        let target = match form {
            TargetForm::Task(id) => seat_of_task(&definition, &id)?,
            TargetForm::Owner(path) => seat_of_owner(&definition, &path)?,
            TargetForm::Fleet => self.open_seats(&definition, sender).await?,
        };
        if let FleetTarget::Seat { session, task } = &target
            && let Some(closed) = self.closed_fleet_seat(*session).await?
        {
            let session = closed
                .successor
                .or(closed.coordinator)
                .ok_or(TargetError::NoOpenSeats)?;
            return Ok(Some(FleetTarget::Seat {
                session,
                task: task.clone(),
            }));
        }
        Ok(Some(target))
    }

    async fn open_seats(
        self,
        definition: &FleetDefinition,
        sender: Uuid,
    ) -> Result<FleetTarget, TargetError> {
        let others: Vec<Uuid> = seat_sessions(definition)
            .into_iter()
            .filter(|s| *s != sender)
            .collect();
        let open = self.open_sessions_among(&others).await?;
        let seats: Vec<Uuid> = others.into_iter().filter(|s| open.contains(s)).collect();
        if seats.is_empty() {
            return Err(TargetError::NoOpenSeats);
        }
        Ok(FleetTarget::Seats(seats))
    }
}
