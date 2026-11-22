use super::PersistError;
use super::session_locks;
use crate::store::models::InterceptedRequest;

pub fn is_subagent_origin(turn_origin: &str) -> bool {
    matches!(turn_origin, "subagent" | "workflow_subagent")
}

pub async fn attribute_inline_agent_instance(
    tx: &mut sqlx::PgConnection,
    req: &InterceptedRequest,
    session_db_id: i64,
    request_db_id: i64,
) -> Result<Option<i64>, PersistError> {
    if !is_subagent_origin(&req.turn_origin) || req.agent_instance_id.is_some() {
        return Ok(req.agent_instance_id);
    }
    let instance = match linked_instance(req) {
        Some(instance_id) => Some(Instance::claimed(instance_id)),
        None => find_or_create_instance(tx, req, session_db_id, request_db_id).await?,
    };
    if let Some(instance) = instance {
        bind_request_to_instance(tx, request_db_id, instance).await?;
    }
    Ok(instance.map(|i| i.id))
}

/// An agent instance a subagent request belongs to. `maybe_unclaimed` is set only
/// when the instance may still lack its first request, so the claim UPDATE runs
/// only then: an instance found through its first request, created with one, or
/// continued from a predecessor request already has it.
#[derive(Debug, Clone, Copy)]
struct Instance {
    id: i64,
    maybe_unclaimed: bool,
}

impl Instance {
    fn claimed(id: i64) -> Self {
        Self {
            id,
            maybe_unclaimed: false,
        }
    }

    fn maybe_unclaimed(id: i64) -> Self {
        Self {
            id,
            maybe_unclaimed: true,
        }
    }
}

fn linked_instance(req: &InterceptedRequest) -> Option<i64> {
    req.thread
        .as_ref()
        .filter(|link| link.continues())
        .and_then(|link| link.predecessor_instance)
}

async fn find_or_create_instance(
    tx: &mut sqlx::PgConnection,
    req: &InterceptedRequest,
    session_db_id: i64,
    request_db_id: i64,
) -> Result<Option<Instance>, PersistError> {
    let persona = req
        .subagent_persona
        .as_deref()
        .filter(|p| !p.trim().is_empty());
    let agent_id = req
        .claude_code_agent_id
        .as_deref()
        .filter(|a| !a.trim().is_empty());
    let Some(agent_id) = agent_id else {
        return match (persona, req.turn_origin.as_str()) {
            (Some(persona), "subagent") => {
                Ok(latest_instance_for_persona(tx, session_db_id, persona)
                    .await?
                    .map(Instance::maybe_unclaimed))
            }
            _ => Ok(None),
        };
    };
    session_locks::lock_agent_instance_binding(&mut *tx, session_db_id).await?;
    if let Some(instance_id) =
        instance_for_agent_run(tx, session_db_id, agent_id, &req.turn_origin).await?
    {
        return Ok(Some(Instance::claimed(instance_id)));
    }
    if let Some(persona) = persona
        && let Some(instance_id) =
            unclaimed_instance_for_persona(tx, session_db_id, persona).await?
    {
        return Ok(Some(Instance::maybe_unclaimed(instance_id)));
    }
    create_instance(tx, session_db_id, persona, request_db_id)
        .await
        .map(|id| Some(Instance::claimed(id)))
}

async fn instance_for_agent_run(
    tx: &mut sqlx::PgConnection,
    session_db_id: i64,
    agent_id: &str,
    turn_origin: &str,
) -> Result<Option<i64>, PersistError> {
    Ok(sqlx::query_scalar!(
        "SELECT ai.id
           FROM agents.instances ai
           JOIN requests.requests first_req ON first_req.id = ai.first_request_id
           JOIN requests.turn_origins tor ON tor.id = first_req.turn_origin_id
          WHERE ai.session_id = $1
            AND first_req.claude_code_agent_id = $2
            AND tor.code = $3
          ORDER BY ai.id
          LIMIT 1",
        session_db_id,
        agent_id,
        turn_origin,
    )
    .fetch_optional(&mut *tx)
    .await?)
}

async fn create_instance(
    tx: &mut sqlx::PgConnection,
    session_db_id: i64,
    persona: Option<&str>,
    request_db_id: i64,
) -> Result<i64, PersistError> {
    Ok(sqlx::query_scalar!(
        "INSERT INTO agents.instances (session_id, subagent_persona, first_request_id, started_at)
         SELECT $1, $2, r.id, r.created_at
           FROM requests.requests r
          WHERE r.id = $3
         RETURNING id",
        session_db_id,
        persona,
        request_db_id,
    )
    .fetch_one(&mut *tx)
    .await?)
}

async fn unclaimed_instance_for_persona(
    tx: &mut sqlx::PgConnection,
    session_db_id: i64,
    persona: &str,
) -> Result<Option<i64>, PersistError> {
    Ok(sqlx::query_scalar!(
        "SELECT ai.id
           FROM agents.instances ai
          WHERE ai.session_id       = $1
            AND ai.subagent_persona = $2
            AND ai.first_request_id IS NULL
          ORDER BY ai.id
          LIMIT 1",
        session_db_id,
        persona,
    )
    .fetch_optional(&mut *tx)
    .await?)
}

async fn latest_instance_for_persona(
    tx: &mut sqlx::PgConnection,
    session_db_id: i64,
    persona: &str,
) -> Result<Option<i64>, PersistError> {
    Ok(sqlx::query_scalar!(
        "SELECT ai.id
           FROM agents.instances ai
          WHERE ai.session_id      = $1
            AND ai.subagent_persona = $2
          ORDER BY (ai.first_request_id IS NULL),
                   ai.first_request_id DESC,
                   ai.id DESC
          LIMIT 1",
        session_db_id,
        persona,
    )
    .fetch_optional(&mut *tx)
    .await?)
}

async fn bind_request_to_instance(
    tx: &mut sqlx::PgConnection,
    request_db_id: i64,
    instance: Instance,
) -> Result<(), PersistError> {
    sqlx::query!(
        "UPDATE requests.requests
            SET agent_instance_id = $2
          WHERE id = $1
            AND agent_instance_id IS NULL",
        request_db_id,
        instance.id,
    )
    .execute(&mut *tx)
    .await?;
    if !instance.maybe_unclaimed {
        return Ok(());
    }
    sqlx::query!(
        "UPDATE agents.instances
            SET first_request_id = $2,
                started_at = (
                    SELECT created_at FROM requests.requests
                     WHERE id = $2
                )
          WHERE id = $1
            AND first_request_id IS NULL",
        instance.id,
        request_db_id,
    )
    .execute(&mut *tx)
    .await?;
    Ok(())
}
