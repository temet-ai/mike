use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use uuid::Uuid;

use crate::store::persistence::{PersistError, PersistHandle};

use super::digest::RelatedSessionContext;

pub async fn fetch_related_contexts(
    persist: &PersistHandle,
    session_uuid: Uuid,
    session_id: &str,
    has_vector: bool,
) -> (Vec<Uuid>, Vec<RelatedSessionContext>) {
    let neighbours = if has_vector {
        match persist
            .knowledge()
            .nearest_sessions(session_uuid, 5, true, 0.5)
            .await
        {
            Ok(n) => n,
            Err(e) => {
                log_nearest_sessions_failed(session_id, &e);
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    let neighbour_ids: Vec<Uuid> = neighbours.iter().map(|n| n.session_id).collect();
    let knowledge_by_neighbour =
        fetch_neighbour_knowledge(persist, &neighbour_ids, session_id).await;

    let mut related_contexts = Vec::with_capacity(neighbours.len());
    for n in &neighbours {
        let overview = fetch_neighbour_overview(persist, n.session_id, session_id).await;
        related_contexts.push(RelatedSessionContext {
            title: n.title.clone(),
            last_seen: n.last_seen,
            outcome: n.outcome.clone(),
            similarity: n.similarity,
            overview,
            learnings: knowledge_by_neighbour
                .get(&n.session_id)
                .cloned()
                .unwrap_or_default(),
        });
    }

    (neighbour_ids, related_contexts)
}

fn log_nearest_sessions_failed(session_id: &str, e: &PersistError) {
    tracing::warn!(session_id, error = %e, "session_judge: nearest_sessions failed");
}

async fn fetch_neighbour_knowledge(
    persist: &PersistHandle,
    neighbour_ids: &[Uuid],
    session_id: &str,
) -> std::collections::HashMap<Uuid, Vec<String>> {
    if neighbour_ids.is_empty() {
        return std::collections::HashMap::new();
    }
    match persist
        .knowledge()
        .fetch_session_knowledge_many(neighbour_ids)
        .await
    {
        Ok(rows) => rows
            .into_iter()
            .map(|k| (k.session_id, k.learnings))
            .collect(),
        Err(e) => {
            log_neighbour_knowledge_fetch_failed(session_id, &e);
            std::collections::HashMap::new()
        }
    }
}

fn log_neighbour_knowledge_fetch_failed(session_id: &str, e: &PersistError) {
    tracing::warn!(session_id, error = %e, "session_judge: neighbour knowledge fetch failed");
}

async fn fetch_neighbour_overview(
    persist: &PersistHandle,
    neighbour_id: Uuid,
    session_id: &str,
) -> Option<String> {
    match persist
        .resolve_session_db_id(&neighbour_id.to_string())
        .await
    {
        Ok(Some(db_id)) => match persist.fetch_session_overview_text(db_id).await {
            Ok(Some((text, _))) => Some(text),
            Ok(None) => None,
            Err(e) => {
                log_neighbour_overview_fetch_failed(session_id, neighbour_id, &e);
                None
            }
        },
        Ok(None) => None,
        Err(e) => {
            log_neighbour_resolve_failed(session_id, neighbour_id, &e);
            None
        }
    }
}

fn log_neighbour_overview_fetch_failed(session_id: &str, neighbour_id: Uuid, e: &PersistError) {
    tracing::warn!(
        session_id,
        neighbour_id = %neighbour_id,
        error = %e,
        "session_judge: neighbour overview fetch failed"
    );
}

fn log_neighbour_resolve_failed(session_id: &str, neighbour_id: Uuid, e: &PersistError) {
    tracing::warn!(
        session_id,
        neighbour_id = %neighbour_id,
        error = %e,
        "session_judge: neighbour session resolve failed"
    );
}
