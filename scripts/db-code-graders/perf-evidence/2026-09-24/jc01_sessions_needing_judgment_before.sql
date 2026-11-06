SELECT s.id AS session_db_id, s.session_uuid::text AS session_id,
       (judged.session_id IS NULL OR (judged.is_unknown AND turned.last_turn_at > judged.judged_at)) AS needs_verdict
  FROM sessions.sessions s
  LEFT JOIN (
       SELECT DISTINCT ON (v.session_id) v.session_id, v.judged_at, o.code = 'unknown' AS is_unknown
         FROM session_judges.verdicts v JOIN session_judges.outcomes o ON o.id = v.outcome_id
        WHERE v.judge_model = 'model-x' AND v.rubric_version = 1
        ORDER BY v.session_id, v.judged_at DESC
  ) judged ON judged.session_id = s.id
  LEFT JOIN (
       SELECT t.session_id, max(t.created_at) AS last_turn_at
         FROM memory.operator_turns t
        
        GROUP BY t.session_id
  ) turned ON turned.session_id = s.id
  LEFT JOIN session_judges.attempts att ON att.session_id = s.id AND att.judge_model = 'model-x' AND att.rubric_version = 1
 WHERE s.last_seen < now() AND turned.session_id IS NOT NULL
   AND (att.session_id IS NULL OR turned.last_turn_at > att.last_attempt_at
        OR (att.consecutive_failures > 0 AND att.last_attempt_at + least(interval '5 minutes' * power(2, least(att.consecutive_failures, 10) - 1), interval '24 hours') < now()))
   AND (judged.session_id IS NULL OR (judged.is_unknown AND turned.last_turn_at > judged.judged_at) OR s.overview IS NULL)
 ORDER BY s.last_seen DESC LIMIT 50
