#[cfg(test)]
mod checkouts_db_tests;
mod documents;
mod embeddings;
mod guidance;
mod machine_locations;
pub(crate) use embeddings::{
    KNOWLEDGE_COUNT_CAP, KnowledgeEmbeddingSource, KnowledgeStateCounts, PENDING_BATCH,
};
mod health;
mod lesson_sync;
mod location_view;
mod locations;
mod nearest_first;
mod reference;
#[cfg(test)]
mod reference_tests;
mod repo_adoption;
mod rerank_passages;
#[cfg(test)]
mod rerank_passages_tests;
mod resolve;
#[cfg(test)]
mod resolve_tests;
mod search;
mod session_binding;
mod session_scope;

pub use documents::{DocumentInput, RegisteredSource, ScanInput, SourceInput};
pub(crate) use guidance::{AGENTS_SOURCE, MEMORY_SOURCE, RULES_SOURCE, SKILLS_SOURCE};
pub use guidance::{GuidanceRow, ShownGuidance};
pub use lesson_sync::LessonImport;
pub(crate) use location_view::is_scratch;
pub use locations::{LocationInput, ProjectIdentity};
pub(crate) use repo_adoption::RepoLocationAdoption;
pub use search::KnowledgeQuery;
pub use search::SemanticCut;
pub(crate) use search::cut_for;
pub(crate) use search::search_page;

use super::PersistError;

fn invalid(message: &str) -> PersistError {
    PersistError::DataInvariant(message.to_string())
}

#[cfg(test)]
mod health_tests;
#[cfg(test)]
mod session_binding_tests;
#[cfg(test)]
mod tests;
