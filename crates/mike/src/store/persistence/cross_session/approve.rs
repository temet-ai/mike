use chrono::{DateTime, Utc};
use serde::Serialize;
use sha2::{Digest as _, Sha256};

use super::super::{PersistError, PersistHandle};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, sqlx::FromRow)]
pub struct ApprovedLesson {
    pub lesson_id: i64,
    pub project_id: i64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ApproveDirectiveProposal<'a> {
    pub id: i64,
    pub text: &'a str,
    pub kind: &'a str,
    pub target_project_ids: &'a [i64],
    pub expires_at: Option<DateTime<Utc>>,
}

impl PersistHandle {
    pub async fn approve_directive_proposal(
        &self,
        p: &ApproveDirectiveProposal<'_>,
    ) -> Result<Vec<ApprovedLesson>, PersistError> {
        crate::metrics::time_db("approve_directive_proposal", async move {
            let normalised = p.text.trim().to_lowercase();
            let text_sha256: Vec<u8> = Sha256::digest(normalised.as_bytes()).to_vec();
            let rows = sqlx::query_file_as!(
                ApprovedLesson,
                "src/store/persistence/cross_session/approve.sql",
                p.id,
                p.text,
                p.target_project_ids,
                text_sha256.as_slice(),
                p.expires_at,
                p.kind,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }
}
