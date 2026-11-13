WITH ends AS (
    SELECT DISTINCT ON (t.session_id) t.session_id, t.id AS end_turn_id, t.closed_at, f.next_state
      FROM memory.operator_turns t
      JOIN memory.turn_facts f ON f.operator_turn_id = t.id
      JOIN requests.requests r ON r.id = t.start_request_id
     WHERE f.prompt_kind = 'genuine' AND r.claude_code_agent_id IS NULL
       AND t.closed_at >= $2 AND t.closed_at < $3
     ORDER BY t.session_id, t.start_request_id DESC
)
SELECT s.id AS session_db_id, s.session_uuid::text AS "session_uuid!", e.end_turn_id AS "end_turn_id!", e.closed_at AS "closed_at?"
  FROM ends e
  JOIN sessions.sessions s ON s.id = e.session_id
  LEFT JOIN session_judges.attempts att
         ON att.session_id = s.id AND att.judge_model = $5 AND att.rubric_version = 3
 WHERE e.next_state = 'none_session_settled'
   AND NOT EXISTS (
       SELECT 1 FROM session_judges.verdicts v
        WHERE v.mode = 'live' AND v.end_turn_id = e.end_turn_id AND v.bundle_id = $1)
   AND (att.session_id IS NULL OR att.consecutive_failures < 5)
 ORDER BY e.closed_at
 LIMIT $4
