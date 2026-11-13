use crate::store::persistence::lessons_repo::LessonsPersist as _;
use std::collections::{HashMap, HashSet};

use crate::analysis::EmbeddingService;
use crate::lesson_cache::relevance::{VectorsKey, eligible_ids};
use crate::lesson_cache::{LessonRelevance, LessonSnapshot};
use crate::store::persistence::{PersistError, PersistHandle};

pub use crate::lesson_cache::outcome;

async fn refresh_outcomes(
    relevance: &LessonRelevance,
    persist: &PersistHandle,
    eligible: &HashSet<i64>,
) {
    if !relevance.outcomes_due().await {
        return;
    }
    let ids: Vec<i64> = eligible.iter().copied().collect();
    match persist.lessons().lesson_outcomes(&ids).await {
        Ok(rows) => relevance.replace_outcomes(rows).await,
        Err(error) => {
            tracing::warn!(%error, "lesson outcome records unavailable; ranking by match alone");
            relevance.touch_outcomes().await;
        }
    }
}

pub async fn refresh(
    relevance: &LessonRelevance,
    service: &EmbeddingService,
    persist: &PersistHandle,
    snapshot: &LessonSnapshot,
) {
    let eligible = eligible_ids(snapshot);
    refresh_outcomes(relevance, persist, &eligible).await;
    if !service.is_available() {
        relevance.forget_vectors().await;
        return;
    }
    let model = format!("{}:{}", service.url(), service.model_name());
    let changed = match load_changed_vectors(relevance, persist, &service.model_name(), eligible)
        .await
    {
        Ok(changed) => changed,
        Err(error) => {
            tracing::warn!(%error, "lesson relevance vectors unavailable; using lexical ranking");
            return;
        }
    };
    let pending = relevance.take_pending(&model, changed).await;
    embed_pending_queries(relevance, service, &model, pending).await;
}

async fn load_changed_vectors(
    relevance: &LessonRelevance,
    persist: &PersistHandle,
    model_name: &str,
    eligible: HashSet<i64>,
) -> Result<Option<(VectorsKey, HashMap<i64, Vec<f32>>)>, PersistError> {
    let fingerprint = persist
        .lessons()
        .injectable_lesson_vectors_fingerprint()
        .await?;
    let key = VectorsKey {
        model_name: model_name.to_owned(),
        fingerprint,
        eligible,
    };
    if relevance.holds_vectors(&key).await {
        return Ok(None);
    }
    let documents = persist
        .lessons()
        .injectable_lesson_vectors(model_name)
        .await?
        .into_iter()
        .filter(|(id, _)| key.eligible.contains(id))
        .collect();
    Ok(Some((key, documents)))
}

async fn embed_pending_queries(
    relevance: &LessonRelevance,
    service: &EmbeddingService,
    model: &str,
    pending: Vec<(String, String, String)>,
) {
    for (_, key, query) in pending {
        match service.embed_query(&query).await {
            Ok(vector) => {
                let current = format!("{}:{}", service.url(), service.model_name());
                if !relevance.cache_query(&current, model, key, vector).await {
                    return;
                }
            }
            Err(error) => {
                tracing::warn!(%error, "lesson query embedding failed; lexical ranking remains available")
            }
        }
    }
}
