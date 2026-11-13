use super::PersistError;
use super::{judge, rehydrate};

impl crate::store::persistence::eval_repo::EvalDb<'_> {
    pub(crate) async fn fetch_workflow_runs(
        self,
        session_db_id: i64,
    ) -> Result<Vec<rehydrate::WorkflowRunRow>, PersistError> {
        crate::metrics::time_db(
            "fetch_workflow_runs",
            rehydrate::fetch_workflow_runs(&self.pool, session_db_id),
        )
        .await
    }
    pub(crate) async fn fetch_workflow_agents(
        self,
        session_db_id: i64,
    ) -> Result<Vec<rehydrate::WorkflowAgentRow>, PersistError> {
        crate::metrics::time_db(
            "fetch_workflow_agents",
            rehydrate::fetch_workflow_agents(&self.pool, session_db_id),
        )
        .await
    }

    pub(crate) async fn fetch_sessions_needing_judgment(
        self,
        query: &judge::NeedsJudgmentQuery<'_>,
    ) -> Result<Vec<judge::SessionForJudgment>, PersistError> {
        crate::metrics::time_db(
            "fetch_sessions_needing_judgment",
            judge::fetch_sessions_needing_judgment(&self.pool, query),
        )
        .await
    }
    pub(crate) async fn fetch_judge_digest(
        self,
        session_db_id: i64,
    ) -> Result<Vec<judge::JudgeDigestRow>, PersistError> {
        crate::metrics::time_db(
            "fetch_judge_digest",
            judge::fetch_judge_digest(&self.pool, session_db_id),
        )
        .await
    }
    pub(crate) async fn insert_session_verdict(
        self,
        params: judge::SessionVerdictInsert<'_>,
    ) -> Result<i64, PersistError> {
        crate::metrics::time_db(
            "insert_session_verdict",
            judge::insert_session_verdict(&self.pool, params),
        )
        .await
    }
    pub async fn record_judge_attempt(
        self,
        session_db_id: i64,
        judge_model: &str,
        rubric_version: i32,
        attempted_at: chrono::DateTime<chrono::Utc>,
        failed: bool,
    ) -> Result<i32, PersistError> {
        crate::metrics::time_db(
            "record_judge_attempt",
            judge::record_judge_attempt(
                &self.pool,
                session_db_id,
                judge_model,
                rubric_version,
                attempted_at,
                failed,
            ),
        )
        .await
    }
    pub(crate) async fn fetch_session_quality(
        self,
        session_id: &str,
    ) -> Result<Option<judge::SessionQualityRow>, PersistError> {
        crate::metrics::time_db(
            "fetch_session_quality",
            judge::fetch_session_quality(&self.pool, session_id),
        )
        .await
    }
}
