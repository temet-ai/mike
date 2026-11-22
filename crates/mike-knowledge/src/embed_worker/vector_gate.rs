use chrono::{DateTime, Utc};

use crate::analysis::EmbeddingService;
use crate::store::models::RowChangedTable;
use crate::store::persistence::{PersistError, PersistHandle};

use super::vectors::{self, SessionVectorSweepOutcome};

pub(super) const NO_SESSION_VECTOR_OWED: &str = "embed_worker.no_session_vector_owed";
pub(super) const PROJECT_CENTROIDS_CURRENT: &str = "embed_worker.project_centroids_current";
pub(super) const SESSION_VECTOR_SOURCES: &[RowChangedTable] =
    &[RowChangedTable::SessionEmbeddingSources];
pub(super) const SESSION_VECTORS: &[RowChangedTable] = &[RowChangedTable::SessionEmbeddings];
pub(super) const VECTOR_BACKSTOP: chrono::Duration = chrono::Duration::seconds(3_600);

fn backstop(now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    Some(now + VECTOR_BACKSTOP)
}

async fn owed_session_vectors(
    persist: &PersistHandle,
    embedding: &EmbeddingService,
    now: DateTime<Utc>,
) -> Result<SessionVectorSweepOutcome, PersistError> {
    if persist.is_fresh(NO_SESSION_VECTOR_OWED, SESSION_VECTOR_SOURCES, now) {
        return Ok(SessionVectorSweepOutcome::default());
    }
    let ticket = persist.fresh_ticket();
    let (found, outcome) = vectors::refresh_owed_session_vectors(persist, embedding).await?;
    if found == 0 {
        persist.mark_fresh(NO_SESSION_VECTOR_OWED, ticket, backstop(now));
    }
    Ok(outcome)
}

async fn stale_project_centroids(
    persist: &PersistHandle,
    wrote: bool,
    now: DateTime<Utc>,
) -> usize {
    if !wrote && persist.is_fresh(PROJECT_CENTROIDS_CURRENT, SESSION_VECTORS, now) {
        return 0;
    }
    let ticket = persist.fresh_ticket();
    let Some(projects) = vectors::refresh_project_centroids(persist).await else {
        return 0;
    };
    persist.mark_fresh(PROJECT_CENTROIDS_CURRENT, ticket, backstop(now));
    projects
}

pub(super) async fn refresh_session_vectors_when_owed(
    persist: &PersistHandle,
    embedding: &EmbeddingService,
) -> Result<SessionVectorSweepOutcome, PersistError> {
    if !embedding.is_available() {
        return Ok(SessionVectorSweepOutcome::default());
    }
    let now = Utc::now();
    let mut outcome = owed_session_vectors(persist, embedding, now).await?;
    outcome.projects_refreshed = stale_project_centroids(persist, outcome.written() > 0, now).await;
    Ok(outcome)
}

#[cfg(test)]
#[path = "vector_gate_tests.rs"]
mod vector_gate_tests;
