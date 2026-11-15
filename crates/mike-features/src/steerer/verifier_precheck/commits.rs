use std::time::Duration;

use crate::store::persistence::PersistHandle;

const RECENT_COMMITS: i64 = 3;
const SHA_CHARS: usize = 9;
const QUERY_TIMEOUT: Duration = Duration::from_millis(500);

fn line(subject: &str, sha: Option<&str>) -> String {
    match sha.map(|s| crate::llm::decisions::text::head_chars(s, SHA_CHARS)) {
        Some(sha) if !sha.is_empty() => format!("{sha} {subject}"),
        _ => subject.to_owned(),
    }
}

pub async fn recent_commits(persist: &PersistHandle, session_db_id: i64) -> Vec<String> {
    let query = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT subject, after_sha FROM workspace.change_events
          WHERE session_id = $1 AND kind = 'commit'
          ORDER BY id DESC
          LIMIT $2",
    )
    .bind(session_db_id)
    .bind(RECENT_COMMITS)
    .fetch_all(persist.pool());
    match tokio::time::timeout(QUERY_TIMEOUT, query).await {
        Ok(Ok(rows)) => rows
            .iter()
            .map(|(subject, sha)| line(subject, sha.as_deref()))
            .collect(),
        Ok(Err(error)) => {
            tracing::warn!(%error, session_db_id, "verifier pre-check: recent commits read failed");
            Vec::new()
        }
        Err(_) => {
            tracing::warn!(
                session_db_id,
                "verifier pre-check: recent commits read timed out"
            );
            Vec::new()
        }
    }
}
