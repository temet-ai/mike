pub mod analyzer;
pub(crate) mod categories;
pub mod config;
pub(crate) mod corpus;
pub mod embeddings;
pub(crate) mod judge;
pub(crate) mod patterns;

pub use analyzer::AnalyzerRegistry;
pub use config::{AnalysisConfig, CategoryConfig, PatternConfig};
pub use embeddings::{EmbedError, EmbeddingConfig, EmbeddingService};
pub use judge::JUDGE_RUBRIC_VERSION;
pub use patterns::Enforcement;

pub fn export_analysis_corpus(out: &std::path::Path) -> anyhow::Result<()> {
    corpus::write_to_path(out)
}

#[doc(hidden)]
pub fn chunk_text_for_test(text: &str, max_chars: usize) -> Vec<String> {
    embeddings::chunk_text(text, max_chars)
}
