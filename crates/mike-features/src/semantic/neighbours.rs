use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::analysis::embeddings::EmbeddingService;
use crate::dossier::view::types::AsOfMode;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::semantic::{
    CandidateRow, CandidateSearch, EmbedderRow, FacetVector, ScopeFilter, vector_literal,
};

use super::SemanticError;
use super::builder::Builder;
use super::check::embed_checked;
use super::config::SemanticConfig;
use super::floors::{FloorSet, Scored};
use super::floors_job::floor_set;
use super::retrieve::{QuerySubject, RETRIEVAL_VERSION, RetrievedKind, SemanticQuery, validate};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Neighbour {
    pub kind: RetrievedKind,
    pub item_id: i64,
    pub pair: String,
    pub similarity: f64,
    pub score: f64,
    pub floor_id: i64,
    pub same_session: bool,
    #[serde(flatten)]
    pub row: CandidateRow,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NeighbourSet {
    pub status: &'static str,
    pub retrieval_version: &'static str,
    pub embedder_key: Option<String>,
    pub audience: &'static str,
    pub scope: &'static str,
    pub as_of: DateTime<Utc>,
    pub as_of_mode: &'static str,
    pub query_facets: Vec<String>,
    pub floor_ids: Vec<i64>,
    pub total: usize,
    pub neighbours: Vec<Neighbour>,
}

pub struct Retriever<'a> {
    pub persist: &'a PersistHandle,
    pub embedding: Option<&'a EmbeddingService>,
    pub config: &'a SemanticConfig,
    pub live_consumer: bool,
}

struct Subject {
    facets: Vec<FacetVector>,
    session_id: Option<i64>,
    project_id: Option<i64>,
    turn_id: Option<i64>,
}

fn empty(q: &SemanticQuery, status: &'static str, key: Option<String>) -> NeighbourSet {
    NeighbourSet {
        status,
        retrieval_version: RETRIEVAL_VERSION,
        embedder_key: key,
        audience: q.audience.code(),
        scope: q.scope.code(),
        as_of: q.as_of.at,
        as_of_mode: q.as_of.mode.code(),
        query_facets: Vec::new(),
        floor_ids: Vec::new(),
        total: 0,
        neighbours: Vec::new(),
    }
}

fn score_rows(
    kind: RetrievedKind,
    rows: Vec<CandidateRow>,
    floors: &FloorSet,
    session: Option<i64>,
) -> Vec<Neighbour> {
    let allowed = kind.pairs();
    let mut best: HashMap<i64, Neighbour> = HashMap::new();
    for row in rows {
        let pair = format!("{}:{}", row.query_facet, row.item_facet);
        if !allowed.contains(&pair.as_str()) {
            continue;
        }
        let Ok(Scored {
            similarity,
            score,
            floor_id,
        }) = floors.score(&pair, row.similarity)
        else {
            continue;
        };
        if score < 0.0 || best.get(&row.item_id).is_some_and(|b| b.score >= score) {
            continue;
        }
        best.insert(
            row.item_id,
            Neighbour {
                kind,
                item_id: row.item_id,
                pair,
                similarity,
                score,
                floor_id,
                same_session: session.is_some() && row.session_id == session,
                row,
            },
        );
    }
    best.into_values().collect()
}

impl Retriever<'_> {
    async fn closed_subject(
        &self,
        turn_id: i64,
        embedder: &EmbedderRow,
    ) -> Result<Subject, SemanticError> {
        let mut facets = self
            .persist
            .knowledge()
            .semantic_subject_facets(turn_id, &embedder.embedder_key)
            .await?;
        if facets.is_empty()
            && let Some(embedding) = self.embedding
        {
            let builder = Builder {
                persist: self.persist,
                embedding,
                config: self.config,
                embedder,
            };
            builder.ensure_turn_items(turn_id).await?;
            facets = self
                .persist
                .knowledge()
                .semantic_subject_facets(turn_id, &embedder.embedder_key)
                .await?;
        }
        let source = self
            .persist
            .knowledge()
            .semantic_turn_source(turn_id)
            .await?;
        Ok(Subject {
            facets,
            session_id: source.as_ref().map(|s| s.session_id),
            project_id: source.and_then(|s| s.project_id),
            turn_id: Some(turn_id),
        })
    }

    async fn open_subject(
        &self,
        session_id: i64,
        prompt: &str,
        ending: &str,
    ) -> Result<Subject, SemanticError> {
        let embedding = self.embedding.ok_or(SemanticError::Disabled)?;
        let texts = vec![prompt.to_owned(), ending.to_owned()];
        let (_, vectors) =
            embed_checked(embedding, &texts, self.config.allow_remote_embedding).await?;
        let facets = ["prompt", "ending"]
            .iter()
            .zip(&vectors)
            .zip(&texts)
            .filter(|(_, text)| !text.trim().is_empty())
            .map(|((facet, v), _)| FacetVector {
                facet: (*facet).to_owned(),
                vector: vector_literal(v),
            })
            .collect();
        Ok(Subject {
            facets,
            session_id: Some(session_id),
            project_id: self
                .persist
                .knowledge()
                .semantic_session_project(session_id)
                .await?,
            turn_id: None,
        })
    }

    pub async fn neighbours(&self, q: &SemanticQuery) -> Result<NeighbourSet, SemanticError> {
        validate(q, self.live_consumer)?;
        let Some(embedder) = self.persist.knowledge().current_semantic_embedder().await? else {
            return Ok(empty(q, "no_embedder", None));
        };
        let subject = match &q.subject {
            QuerySubject::ClosedTurn(id) => self.closed_subject(*id, &embedder).await?,
            QuerySubject::OpenTurn {
                session_id,
                prompt_text,
                ending_text,
            } => {
                self.open_subject(*session_id, prompt_text, ending_text)
                    .await?
            }
        };
        let key = Some(embedder.embedder_key.clone());
        if subject.facets.is_empty() {
            return Ok(empty(q, "no_items", key));
        }
        let floors = floor_set(
            self.persist,
            &embedder.embedder_key,
            self.config.floor_quantile,
        )
        .await?;
        let wanted: Vec<&str> = q
            .kinds
            .iter()
            .flat_map(|k| k.pairs().iter().copied())
            .collect();
        if !floors.covers_any(&wanted) {
            return Ok(empty(q, "no_floor", key));
        }
        let mut all = Vec::new();
        for kind in &q.kinds {
            let rows = self
                .persist
                .knowledge()
                .semantic_candidates(kind.code(), &self.search(q, &embedder, &subject))
                .await?;
            all.extend(score_rows(*kind, rows, &floors, subject.session_id));
        }
        all.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.item_id.cmp(&b.item_id)));
        let total = all.len();
        let neighbours: Vec<Neighbour> = all.into_iter().skip(q.offset).take(q.k).collect();
        Ok(NeighbourSet {
            query_facets: subject.facets.iter().map(|f| f.facet.clone()).collect(),
            floor_ids: floors.ids(),
            total,
            neighbours,
            ..empty(q, "ok", key)
        })
    }

    fn search<'s>(
        &self,
        q: &SemanticQuery,
        embedder: &'s EmbedderRow,
        subject: &'s Subject,
    ) -> CandidateSearch<'s> {
        CandidateSearch {
            embedder_key: &embedder.embedder_key,
            facets: &subject.facets,
            as_of: q.as_of.at,
            reconstructed: q.as_of.mode == AsOfMode::Reconstructed,
            audience: q.audience.code(),
            scope: ScopeFilter {
                scope: q.scope.code(),
                project_id: subject.project_id,
                session_id: subject.session_id,
            },
            subject_turn: subject.turn_id,
            limit: i64::try_from((q.k + q.offset).max(1) * 20)
                .unwrap_or(200)
                .min(500),
            embedding_model: &embedder.model,
        }
    }
}
