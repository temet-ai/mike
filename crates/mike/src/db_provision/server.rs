use std::future::Future;

use crate::store::persistence::{migrations, pool};

#[derive(Debug, thiserror::Error)]
pub(crate) enum ServerError {
    #[error("password authentication failed: {0}")]
    Auth(String),
    #[error("{0}")]
    Other(String),
}

pub(crate) trait Server {
    fn migrate(&self, url: &str) -> impl Future<Output = Result<usize, ServerError>>;
}

pub(crate) struct SqlxServer;

impl Server for SqlxServer {
    async fn migrate(&self, url: &str) -> Result<usize, ServerError> {
        let pool = pool::build_pool(url).await.map_err(|e| classify(&e))?;
        let assessment = migrations::assess(&pool).await.map_err(|e| classify(&e));
        pool.close().await;
        let assessment = assessment?;
        if assessment.is_blocking() {
            return Err(ServerError::Other(format!(
                "the database and this build of mike disagree about the migrations: {}",
                assessment.not_current_reason().unwrap_or_default()
            )));
        }
        let pending = assessment.pending.len();
        if pending > 0 {
            migrations::run_patient(url)
                .await
                .map_err(|e| classify(&e))?;
        }
        Ok(pending)
    }
}

pub(crate) fn classify(error: &dyn std::error::Error) -> ServerError {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(inner) = source {
        text.push_str(": ");
        text.push_str(&inner.to_string());
        source = inner.source();
    }
    if text.contains("password authentication failed") {
        ServerError::Auth(text)
    } else {
        ServerError::Other(text)
    }
}
