use serde::Serialize;

use crate::dossier::view::types::AsOf;
use crate::store::persistence::PersistHandle;

use super::SemanticError;
use super::config::SemanticConfig;
use super::neighbours::{NeighbourSet, Retriever};
use super::retrieve::{Audience, QuerySubject, RetrievedKind, Scope, SemanticQuery};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SemanticSections {
    pub status: &'static str,
    pub corrections: Option<NeighbourSet>,
    pub directives: Option<NeighbourSet>,
    pub lessons: Option<NeighbourSet>,
}

fn scope_of(config: &SemanticConfig) -> Scope {
    match config.retrieval_scope {
        super::config::RetrievalScopeSetting::Operator => Scope::Operator,
        super::config::RetrievalScopeSetting::Project => Scope::Project,
        super::config::RetrievalScopeSetting::Session => Scope::Session,
    }
}

async fn semantic_enabled(persist: &PersistHandle) -> Result<bool, SemanticError> {
    let rows = persist.fetch_feature_toggles().await?;
    Ok(rows
        .iter()
        .find(|(k, _, _)| k == "semantic_context")
        .is_none_or(|(_, enabled, _)| *enabled))
}

pub async fn steerer_sections(
    persist: &PersistHandle,
    turn_id: i64,
    as_of: AsOf,
) -> Result<SemanticSections, SemanticError> {
    if !semantic_enabled(persist).await? {
        return Ok(SemanticSections {
            status: "disabled",
            corrections: None,
            directives: None,
            lessons: None,
        });
    }
    let config = SemanticConfig::from_rows(&persist.fetch_settings().await?);
    let retriever = Retriever {
        persist,
        embedding: None,
        config: &config,
        live_consumer: true,
    };
    let query = |kind| SemanticQuery {
        subject: QuerySubject::ClosedTurn(turn_id),
        audience: Audience::Steerer,
        kinds: vec![kind],
        scope: scope_of(&config),
        as_of,
        k: config.retrieval_k,
        offset: 0,
    };
    Ok(SemanticSections {
        status: "ok",
        corrections: Some(
            retriever
                .neighbours(&query(RetrievedKind::Correction))
                .await?,
        ),
        directives: Some(
            retriever
                .neighbours(&query(RetrievedKind::Directive))
                .await?,
        ),
        lessons: Some(retriever.neighbours(&query(RetrievedKind::Lesson)).await?),
    })
}

fn day(n: &super::neighbours::Neighbour) -> String {
    n.row.effective_at.format("%Y-%m-%d").to_string()
}

pub fn lines(set: Option<&NeighbourSet>, label: &str) -> Vec<String> {
    let Some(set) = set else {
        return Vec::new();
    };
    set.neighbours
        .iter()
        .map(|n| {
            let place = if n.same_session {
                "this session"
            } else {
                "another session"
            };
            format!(
                "- {label} {} ({place}, {}; similarity {:.2}): {}",
                n.item_id,
                day(n),
                n.similarity,
                n.row.text
            )
        })
        .collect()
}
