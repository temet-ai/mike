pub mod analyzer;
pub mod categories;
pub mod config;
pub mod corpus;
pub mod embeddings;
pub mod patterns;
pub mod rerank;

pub use analyzer::AnalyzerRegistry;
pub use config::{AnalysisConfig, CategoryConfig, PatternConfig};
pub use embeddings::{EmbedError, EmbeddingConfig, EmbeddingService};
pub use patterns::Enforcement;
pub use rerank::{RerankConfig, RerankError, RerankService};

pub fn export_analysis_corpus(out: &std::path::Path) -> anyhow::Result<()> {
    corpus::write_to_path(out)
}

#[doc(hidden)]
pub fn chunk_text_for_test(text: &str, max_chars: usize) -> Vec<String> {
    embeddings::chunk_text(text, max_chars)
}
