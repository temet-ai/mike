use uuid::Uuid;

use crate::api::handlers::{SessionSummary, compute_session_busy};
use crate::config::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet_correlation::ActorEnrichment;
use crate::workspace::Liveness;

async fn resolve_actor_session_db_id(handle: &PersistHandle, session_uuid: &str) -> Option<i64> {
    match handle.resolve_session_db_id(session_uuid).await {
        Ok(Some(id)) => Some(id),
        Ok(None) => None,
        Err(e) => {
            tracing::warn!(error = %e, session_uuid = %session_uuid, "fleet join: session_db_id resolve failed");
            None
        }
    }
}

async fn fetch_actor_session_record(
    handle: &PersistHandle,
    session_db_id: i64,
    session_uuid: &str,
) -> Option<SessionSummary> {
    match handle.fetch_session_summary(session_db_id).await {
        Ok(Some(row)) => Some(SessionSummary::from(&row)),
        Ok(None) => None,
        Err(e) => {
            tracing::warn!(error = %e, session_uuid = %session_uuid, "fleet join: session summary fetch failed");
            None
        }
    }
}

async fn fetch_actor_enrichment_or_default(
    handle: &PersistHandle,
    session_db_id: i64,
    session_uuid: &str,
) -> ActorEnrichment {
    match handle.fetch_actor_enrichment(session_db_id).await {
        Ok(enrichment) => enrichment,
        Err(e) => {
            tracing::warn!(error = %e, session_uuid = %session_uuid, "fleet join: actor enrichment fetch failed");
            ActorEnrichment::default()
        }
    }
}

async fn fetch_registration_liveness(
    handle: &PersistHandle,
    uuid: Uuid,
    session_uuid: &str,
) -> Option<Liveness> {
    match handle.fetch_session_liveness(uuid).await {
        Ok(liveness) => liveness,
        Err(e) => {
            tracing::warn!(error = %e, session_uuid = %session_uuid, "fleet join: registration liveness fetch failed");
            None
        }
    }
}

pub(super) async fn fetch_actor_correlation(
    state: &AppState,
    session_uuid: &str,
) -> Option<(SessionSummary, ActorEnrichment, Option<Liveness>)> {
    let handle = state.persist()?;
    let uuid = Uuid::parse_str(session_uuid).ok()?;
    let session_db_id = resolve_actor_session_db_id(handle, session_uuid).await?;
    let mut summary = fetch_actor_session_record(handle, session_db_id, session_uuid).await?;
    summary.busy = compute_session_busy(
        state.in_flight.is_session_busy(session_uuid),
        &summary.last_status,
    );

    let enrichment = fetch_actor_enrichment_or_default(handle, session_db_id, session_uuid).await;
    let registration_liveness = fetch_registration_liveness(handle, uuid, session_uuid).await;

    Some((summary, enrichment, registration_liveness))
}
