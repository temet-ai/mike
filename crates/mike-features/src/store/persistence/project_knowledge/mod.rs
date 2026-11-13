#[cfg(test)]
pub mod checkouts_db_tests;
pub mod documents;
pub mod embeddings;
pub mod guidance;
pub mod machine_locations;
pub mod memos;
#[cfg(test)]
pub mod memos_tests;
pub mod model_keys;
pub use embeddings::{
    KNOWLEDGE_COUNT_CAP, KnowledgeEmbeddingSource, KnowledgeStateCounts, PENDING_BATCH,
};
pub use model_keys::{ModelKeys, QueryEmbedding};
pub mod health;
pub mod lesson_sync;
pub mod location_view;
pub mod locations;
pub mod nearest_first;
pub mod reference;
#[cfg(test)]
pub mod reference_tests;
pub mod repo_adoption;
pub mod rerank_passages;
#[cfg(test)]
pub mod rerank_passages_tests;
pub mod resolve;
#[cfg(test)]
pub mod resolve_tests;
pub mod search;
pub mod session_binding;
pub mod session_scope;
pub mod window;

pub use documents::{DocumentInput, RegisteredSource, ScanInput, SourceInput};
pub use guidance::{AGENTS_SOURCE, MEMORY_SOURCE, RULES_SOURCE, SKILLS_SOURCE};
pub use guidance::{GuidanceRow, ShownGuidance};
pub use lesson_sync::LessonImport;
pub use location_view::is_scratch;
pub use locations::{LocationInput, ProjectIdentity};
pub use repo_adoption::RepoLocationAdoption;
pub use search::KnowledgeQuery;
pub use search::REFERENCE_NOT_IN_PROJECT;
pub use search::SemanticCut;
pub use search::cut_for;
pub use search::search_page;

use super::PersistError;

fn invalid(message: &str) -> PersistError {
    PersistError::DataInvariant(message.to_string())
}

#[cfg(test)]
pub mod health_tests;
#[cfg(test)]
pub mod search_filters_tests;
#[cfg(test)]
pub mod session_binding_tests;
#[cfg(test)]
mod tests;
