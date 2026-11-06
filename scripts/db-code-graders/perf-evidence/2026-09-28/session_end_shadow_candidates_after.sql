WITH ends AS (
    SELECT DISTINCT ON (t.session_id) t.session_id, t.id AS end_turn_id, t.closed_at, f.next_state
      FROM memory.operator_turns t
      JOIN memory.turn_facts f ON f.operator_turn_id = t.id
      JOIN requests.requests r ON r.id = t.start_request_id
     WHERE f.prompt_kind = 'genuine' AND r.claude_code_agent_id IS NULL
       AND t.closed_at >= now() - make_interval(hours => 72)
     ORDER BY t.session_id, t.start_request_id DESC
)
SELECT s.id AS session_db_id, s.session_uuid::text AS session_uuid, e.end_turn_id
  FROM ends e
  JOIN sessions.sessions s ON s.id = e.session_id
  LEFT JOIN session_judges.attempts att
         ON att.session_id = s.id AND att.judge_model = 'shadow:google/gemini-3.5-flash-lite' AND att.rubric_version = 3
 WHERE e.next_state = 'none_session_settled'
   AND NOT EXISTS (
       SELECT 1 FROM session_judges.verdicts v
        WHERE v.mode = 'shadow' AND v.end_turn_id = e.end_turn_id AND v.bundle_id = 13)
   AND (att.session_id IS NULL OR att.window_started_at IS NULL
        OR att.window_started_at < now() - interval '24 hours' OR att.window_attempts < 3)
   AND (att.session_id IS NULL OR att.consecutive_failures = 0
        OR att.last_attempt_at + least(interval '5 minutes' * power(2, least(att.consecutive_failures, 10) - 1),
                                       interval '24 hours') < now())
 ORDER BY e.closed_at
 LIMIT 50;
