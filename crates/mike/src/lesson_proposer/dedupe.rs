use crate::analysis::embeddings::{EmbedError, EmbeddingService};

pub struct Embedded {
    pub vector: Vec<f32>,
    pub model: String,
}

pub async fn embed_rule(embedding: &EmbeddingService, rule: &str) -> Result<Embedded, EmbedError> {
    let mut vectors = embedding
        .embed_documents(std::slice::from_ref(&rule.to_owned()))
        .await?;
    let vector = vectors.pop().ok_or_else(|| {
        EmbedError::Parse("embed_documents returned no vector for the rule".to_owned())
    })?;
    Ok(Embedded {
        vector,
        model: embedding.model_name(),
    })
}
