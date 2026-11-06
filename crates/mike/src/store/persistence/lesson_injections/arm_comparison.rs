use chrono::{DateTime, Utc};

use super::super::{PersistError, PersistHandle};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArmComparisonRow {
    pub arm: String,
    pub sessions: i64,
    pub operator_turns: i64,
    pub judged_turns: i64,
    pub corrected_turns: i64,
    pub lessons_delivered: i64,
    pub requests: i64,
    pub usage_requests: i64,
    pub input_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
}

impl PersistHandle {
    #[rustfmt::skip]
    pub async fn compare_lesson_delivery_arms(
        &self,
        since: DateTime<Utc>,
        until: DateTime<Utc>,
    ) -> Result<Vec<ArmComparisonRow>, PersistError> {
        crate::metrics::time_db("compare_lesson_delivery_arms", async move {
            let rows = sqlx::query_as!(ArmComparisonRow,
                r#"WITH arm_sessions AS (
    SELECT a.arm, s.id AS session_id
      FROM memory.lesson_delivery_arms a
      JOIN sessions.sessions s ON s.session_uuid = a.session_uuid
), turn_counts AS (
    SELECT x.arm,
           count(*) AS operator_turns,
           count(*) FILTER (WHERE t.reaction IN ('accepted', 'corrected', 'redirected')) AS judged_turns,
           count(*) FILTER (WHERE t.reaction = 'corrected') AS corrected_turns
      FROM arm_sessions x
     -- LATERAL with OFFSET 0: probe the per-session indexes for each arm session, whatever the planner thinks the arms table holds
     CROSS JOIN LATERAL (
            SELECT ot.reaction
              FROM memory.operator_turns ot
              JOIN memory.turn_facts f ON f.operator_turn_id = ot.id
             WHERE ot.session_id = x.session_id
               AND ot.created_at >= $1 AND ot.created_at < $2
               AND f.prompt_kind = 'genuine' AND f.text_chars > 0
            OFFSET 0) t
     GROUP BY x.arm
), delivered AS (
    SELECT x.arm, count(*) AS lessons_delivered
      FROM arm_sessions x
     -- LATERAL with OFFSET 0: same per-session index probe as above
     CROSS JOIN LATERAL (
            SELECT i.id
              FROM memory.lesson_injections i
             WHERE i.session_id = x.session_id
               AND i.injected_at >= $1 AND i.injected_at < $2
            OFFSET 0) i
     GROUP BY x.arm
), usage AS (
    SELECT x.arm,
           count(DISTINCT x.session_id) AS sessions,
           count(*) AS requests,
           count(*) FILTER (WHERE rsp.input_tokens IS NOT NULL) AS usage_requests,
           coalesce(sum(rsp.input_tokens), 0)::bigint AS input_tokens,
           coalesce(sum(rsp.cache_read_input_tokens), 0)::bigint AS cache_read_tokens,
           coalesce(sum(rsp.cache_creation_input_tokens), 0)::bigint AS cache_write_tokens
      FROM arm_sessions x
      JOIN requests.requests r ON r.session_id = x.session_id
      LEFT JOIN responses.responses rsp ON rsp.request_id = r.id
     WHERE r.created_at >= $1 AND r.created_at < $2
     GROUP BY x.arm
)
SELECT arms.arm AS "arm!",
       coalesce(u.sessions, 0) AS "sessions!",
       coalesce(tc.operator_turns, 0) AS "operator_turns!",
       coalesce(tc.judged_turns, 0) AS "judged_turns!",
       coalesce(tc.corrected_turns, 0) AS "corrected_turns!",
       coalesce(d.lessons_delivered, 0) AS "lessons_delivered!",
       coalesce(u.requests, 0) AS "requests!",
       coalesce(u.usage_requests, 0) AS "usage_requests!",
       coalesce(u.input_tokens, 0) AS "input_tokens!",
       coalesce(u.cache_read_tokens, 0) AS "cache_read_tokens!",
       coalesce(u.cache_write_tokens, 0) AS "cache_write_tokens!"
  FROM (VALUES ('mod'), ('control')) AS arms(arm)
  LEFT JOIN turn_counts tc ON tc.arm = arms.arm
  LEFT JOIN delivered d ON d.arm = arms.arm
  LEFT JOIN usage u ON u.arm = arms.arm
 ORDER BY arms.arm DESC"#,
                since, until)
                .fetch_all(&self.pool)
                .await?;
            Ok(rows)
        })
        .await
    }
}
