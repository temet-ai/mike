use chrono::{DateTime, Utc};

use crate::store::persistence::PersistError;

#[derive(Debug, Clone, Copy)]
pub struct StreamWindow {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamSource {
    Stored,
    J2Text,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct StreamRow {
    pub session_id: i64,
    pub request_id: i64,
    pub created_at: DateTime<Utc>,
    pub turn_id: Option<i64>,
    pub kind_code: Option<String>,
    pub rule: Option<String>,
    pub repeat_key: Option<String>,
    pub classifier_version: Option<String>,
    pub text: Option<String>,
    pub author: Option<String>,
    pub has_agent_id: bool,
}

impl crate::store::persistence::dossier_repo::DossierDb<'_> {
    pub async fn load_prompt_streams(
        self,
        session_ids: &[i64],
        window: StreamWindow,
        source: StreamSource,
    ) -> Result<Vec<StreamRow>, PersistError> {
        if session_ids.is_empty() {
            return Ok(Vec::new());
        }
        match source {
            StreamSource::Stored => {
                crate::metrics::time_db("load_prompt_streams_stored", async move {
                    let rows = sqlx::query_file_as!(
                        StreamRow,
                        "src/store/persistence/prompt_stream/stored.sql",
                        session_ids,
                        window.from,
                        window.to,
                    )
                    .fetch_all(&self.pool)
                    .await?;
                    Ok(rows)
                })
                .await
            }
            StreamSource::J2Text => {
                crate::metrics::time_db("load_prompt_streams_j2_text", async move {
                    let rows = sqlx::query_file_as!(
                        StreamRow,
                        "src/store/persistence/prompt_stream/j2_text.sql",
                        session_ids,
                        window.from,
                        window.to,
                    )
                    .fetch_all(&self.pool)
                    .await?;
                    Ok(rows)
                })
                .await
            }
        }
    }
}
