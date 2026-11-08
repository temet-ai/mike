use chrono::{DateTime, Utc};

use super::{
    EndRegistrationOutcome, HEARTBEAT_LEASE_MINUTES, RegisterWorkspaceSession,
    RegisteredWorkspaceSession, RenewedHeartbeat,
};
use crate::store::persistence::{PersistError, PersistHandle};
use crate::workspace::RegisteredVia;

async fn heal_project_assignment(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    session_id: i64,
    project_path: Option<&str>,
) -> Result<i64, PersistError> {
    let assigned = sqlx::query!(
        "SELECT project_id, project_resolution FROM sessions.sessions WHERE id = $1",
        session_id,
    )
    .fetch_one(&mut **tx)
    .await?;
    let (mut project_id, project_resolution) = (assigned.project_id, assigned.project_resolution);

    if let Some(project_path) = project_path.filter(|path| !path.trim().is_empty())
        && matches!(project_resolution.as_str(), "unresolved" | "git")
    {
        project_id = crate::store::persistence::session_projects::upsert_canonical_project(
            &mut *tx,
            project_path.trim(),
        )
        .await?;
        crate::store::persistence::session_projects::repoint_session_project(
            &mut *tx, session_id, project_id, "git",
        )
        .await?;
    }

    Ok(project_id)
}

async fn upsert_registration_row(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    session_id: i64,
    project_id: i64,
    input: &RegisterWorkspaceSession<'_>,
    registered_via_str: &str,
    lease_expires_at: DateTime<Utc>,
) -> Result<(), PersistError> {
    let boot_id = input
        .pid
        .and_then(|_| crate::workspace::boot_id::CURRENT_BOOT_ID.clone());

    sqlx::query!(
        "INSERT INTO workspace.session_registrations
                     (session_id, project_id, cwd, worktree, registered_via,
                      registered_at, lease_expires_at, released_at, pid, boot_id, reaped_at)
                 VALUES ($1, $2, $3, $4, $5, now(), $6, NULL, $7, $8, NULL)
                 ON CONFLICT (session_id) DO UPDATE
                     SET project_id       = excluded.project_id,
                         cwd              = excluded.cwd,
                         worktree         = excluded.worktree,
                         registered_via   = excluded.registered_via,
                         registered_at    = now(),
                         lease_expires_at = excluded.lease_expires_at,
                         released_at      = NULL,
                         -- COALESCE, not overwrite: most re-registrations
                         -- (any hook call until H3 ships) carry no pid, and
                         -- clobbering a previously-known-good pid/boot_id
                         -- with NULL on every such re-register would make a
                         -- reaped row's self-heal (A5) permanently
                         -- non-reapable again.
                         pid              = COALESCE(excluded.pid, workspace.session_registrations.pid),
                         boot_id          = COALESCE(excluded.boot_id, workspace.session_registrations.boot_id),
                         reaped_at        = NULL",
        session_id,
        project_id,
        input.cwd,
        input.worktree,
        registered_via_str,
        lease_expires_at,
        input.pid,
        boot_id,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub(crate) struct LaunchIdentitySeed {
    pub(crate) name: Option<String>,
    pub(crate) role: Option<String>,
    pub(crate) instructions: Option<String>,
    pub(crate) goal: Option<String>,
    pub(crate) next_steps: Option<String>,
}

async fn fetch_pending_launch_identity(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    session_uuid: uuid::Uuid,
) -> Result<Option<LaunchIdentitySeed>, PersistError> {
    let identity: Option<serde_json::Value> = sqlx::query_scalar!(
        r#"SELECT identity AS "identity!"
             FROM launch.launches
            WHERE session_uuid = $1
              AND identity IS NOT NULL
            ORDER BY created_at DESC
            LIMIT 1"#,
        session_uuid,
    )
    .fetch_optional(&mut **tx)
    .await?;
    match identity {
        Some(value) => Ok(Some(serde_json::from_value(value)?)),
        None => Ok(None),
    }
}

async fn apply_session_identity(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    session_id: i64,
    input: &RegisterWorkspaceSession<'_>,
    seed: Option<&LaunchIdentitySeed>,
) -> Result<(), PersistError> {
    let (goal, next_steps) = match input.intent {
        Some((goal, next_steps)) => (Some(goal), Some(next_steps)),
        None => (None, None),
    };
    sqlx::query!(
        "INSERT INTO sessions.session_goals
                (session_id, goal, next_steps, set_by, role, display_name, instructions, updated_at)
            VALUES ($1, COALESCE($2, $7, ''), COALESCE($3, $8, ''), $12,
                    COALESCE($4, $9), COALESCE($5, $10), COALESCE($6, $11), now())
            ON CONFLICT (session_id) DO UPDATE
                SET goal         = COALESCE($2, NULLIF(sessions.session_goals.goal, ''), $7,
                                            sessions.session_goals.goal),
                    next_steps   = COALESCE($3, NULLIF(sessions.session_goals.next_steps, ''), $8,
                                            sessions.session_goals.next_steps),
                    set_by       = excluded.set_by,
                    role         = COALESCE($4, sessions.session_goals.role, $9),
                    display_name = COALESCE($5, sessions.session_goals.display_name, $10),
                    instructions = COALESCE($6, sessions.session_goals.instructions, $11),
                    updated_at   = now()",
        session_id,
        goal,
        next_steps,
        input.role,
        input.name,
        input.instructions,
        seed.and_then(|s| s.goal.as_deref()),
        seed.and_then(|s| s.next_steps.as_deref()),
        seed.and_then(|s| s.role.as_deref()),
        seed.and_then(|s| s.name.as_deref()),
        seed.and_then(|s| s.instructions.as_deref()),
        input.registered_via,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn identity_is_absent(
    input: &RegisterWorkspaceSession<'_>,
    seed: Option<&LaunchIdentitySeed>,
) -> bool {
    let explicit_absent = input.intent.is_none()
        && input.role.is_none()
        && input.name.is_none()
        && input.instructions.is_none();
    let seed_absent = seed.is_none_or(|s| {
        s.goal.is_none()
            && s.next_steps.is_none()
            && s.role.is_none()
            && s.name.is_none()
            && s.instructions.is_none()
    });
    explicit_absent && seed_absent
}

async fn upsert_session_identity(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    session_id: i64,
    input: &RegisterWorkspaceSession<'_>,
) -> Result<(), PersistError> {
    let seed = fetch_pending_launch_identity(tx, input.session_uuid).await?;
    if identity_is_absent(input, seed.as_ref()) {
        return Ok(());
    }
    apply_session_identity(tx, session_id, input, seed.as_ref()).await
}

impl PersistHandle {
    pub async fn register_workspace_session(
        &self,
        input: RegisterWorkspaceSession<'_>,
    ) -> Result<RegisteredWorkspaceSession, PersistError> {
        let registered_via: RegisteredVia = input
            .registered_via
            .parse()
            .map_err(|()| PersistError::InvalidRegisteredVia(input.registered_via.to_string()))?;

        crate::metrics::time_db("workspace_register_session", async move {
            let mut tx = self.pool.begin().await?;

            let (session_id, flow_admission) =
                super::flow_admission::resolve_registration_session(&mut tx, &input).await?;
            sqlx::query!(
                "UPDATE sessions.sessions SET cwd = $2 WHERE id = $1",
                session_id,
                input.cwd,
            )
            .execute(&mut *tx)
            .await?;

            let project_id =
                heal_project_assignment(&mut tx, session_id, input.project_path).await?;

            let resolved = sqlx::query!(
                "SELECT p.project_path, s.project_resolution
                   FROM sessions.sessions s
                   JOIN projects.projects p ON p.id = s.project_id
                  WHERE s.id = $1",
                session_id,
            )
            .fetch_one(&mut *tx)
            .await?;
            let (project_path, project_resolution) =
                (resolved.project_path, resolved.project_resolution);

            let lease_expires_at = Utc::now() + chrono::Duration::minutes(HEARTBEAT_LEASE_MINUTES);
            let registered_via_str = registered_via.as_db_str();

            upsert_registration_row(
                &mut tx,
                session_id,
                project_id,
                &input,
                registered_via_str,
                lease_expires_at,
            )
            .await?;

            upsert_session_identity(&mut tx, session_id, &input).await?;
            super::launch_brief::materialize_launch_briefs(&mut *tx, input.session_uuid).await?;

            let actor =
                super::ledger::resolve_actor_label(&mut *tx, input.session_uuid, session_id)
                    .await?;
            super::ledger::insert_ledger_event(
                &mut *tx,
                project_id,
                Some(session_id),
                &actor,
                "registered",
                &input.session_uuid.to_string(),
            )
            .await?;

            tx.commit().await?;
            Ok(RegisteredWorkspaceSession {
                flow_admission,
                project_id,
                project_path: (project_resolution != "unresolved"
                    && project_path != crate::store::persistence::SENTINEL_PROJECT_PATH)
                    .then_some(project_path),
                project_resolution,
                registered_via: registered_via_str,
                lease_expires_at,
            })
        })
        .await
    }

    pub async fn end_workspace_registration(
        &self,
        session_uuid: uuid::Uuid,
        pid: Option<i32>,
    ) -> Result<EndRegistrationOutcome, PersistError> {
        crate::metrics::time_db("workspace_end_registration", async move {
            let mut tx = self.pool.begin().await?;

            let Some(session_id) = self.session_row_id_in(&mut *tx, session_uuid).await? else {
                return Err(PersistError::RegistrationNotFound(session_uuid));
            };

            let active = sqlx::query!(
                "SELECT project_id, pid
                   FROM workspace.session_registrations
                  WHERE session_id = $1
                    AND released_at IS NULL",
                session_id,
            )
            .fetch_optional(&mut *tx)
            .await?;
            let Some(active) = active else {
                return Err(PersistError::RegistrationNotFound(session_uuid));
            };
            let (project_id, registered_pid) = (active.project_id, active.pid);

            let actor =
                super::ledger::resolve_actor_label(&mut *tx, session_uuid, session_id).await?;

            if let (Some(caller_pid), Some(registered_pid)) = (pid, registered_pid)
                && caller_pid != registered_pid
            {
                super::ledger::insert_ledger_event(
                    &mut *tx,
                    project_id,
                    Some(session_id),
                    &actor,
                    "end_rejected",
                    &session_uuid.to_string(),
                )
                .await?;
                tx.commit().await?;
                return Ok(EndRegistrationOutcome::Rejected);
            }

            sqlx::query!(
                "UPDATE workspace.session_registrations
                    SET released_at = now()
                  WHERE session_id = $1
                    AND released_at IS NULL",
                session_id,
            )
            .execute(&mut *tx)
            .await?;

            let closed_turn_id: Option<i64> = sqlx::query_scalar!(
                "UPDATE memory.operator_turns
                    SET closed_at = now()
                  WHERE session_id = $1
                    AND closed_at IS NULL
              RETURNING id",
                session_id,
            )
            .fetch_optional(&mut *tx)
            .await?;
            if let Some(turn_id) = closed_turn_id {
                crate::store::persistence::turn_lifecycle::materialize_turn_aggregates(
                    &mut tx, turn_id,
                )
                .await?;
                tracing::debug!(
                    session_id = %session_uuid,
                    turn_id,
                    "closed the session's open operator turn on SessionEnd"
                );
            }

            super::ledger::insert_ledger_event(
                &mut *tx,
                project_id,
                Some(session_id),
                &actor,
                "ended",
                &session_uuid.to_string(),
            )
            .await?;

            tx.commit().await?;
            Ok(EndRegistrationOutcome::Ended)
        })
        .await
    }

    pub async fn renew_workspace_lease(
        &self,
        session_uuid: uuid::Uuid,
    ) -> Result<RenewedHeartbeat, PersistError> {
        crate::metrics::time_db("workspace_renew_lease", async move {
            let lease_expires_at = Utc::now() + chrono::Duration::minutes(HEARTBEAT_LEASE_MINUTES);
            let row = sqlx::query!(
                "UPDATE workspace.session_registrations r
                    SET lease_expires_at = $2
                   FROM sessions.sessions s
                  WHERE s.id = r.session_id
                    AND s.session_uuid = $1
                    AND r.released_at IS NULL
              RETURNING r.session_id",
                session_uuid,
                lease_expires_at,
            )
            .fetch_optional(&self.pool)
            .await?;
            match row {
                Some(r) => Ok(RenewedHeartbeat {
                    session_id: r.session_id,
                    lease_expires_at,
                }),
                None => Err(PersistError::RegistrationNotFound(session_uuid)),
            }
        })
        .await
    }
}
