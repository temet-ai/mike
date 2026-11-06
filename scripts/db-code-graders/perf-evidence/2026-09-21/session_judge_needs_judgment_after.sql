WITH attempts AS (
  SELECT DISTINCT ON (v.session_id) v.session_id, v.judged_at AS last_attempt_at, 0 AS consecutive_failures
    FROM session_judges.verdicts v JOIN session_judges.outcomes o ON o.id = v.outcome_id
   WHERE v.judge_model = 'google/gemini-3.5-flash-lite' AND v.rubric_version = 2 AND o.code = 'unknown'
   ORDER BY v.session_id, v.judged_at DESC, v.id DESC)
SELECT s.id AS session_db_id, s.session_uuid::text AS session_id,
       (judged.session_id IS NULL OR (judged.is_unknown AND turned.last_turn_at > judged.judged_at)) AS needs_verdict
  FROM sessions.sessions s
  LEFT JOIN (SELECT DISTINCT ON (v.session_id) v.session_id, v.judged_at, o.code = 'unknown' AS is_unknown FROM session_judges.verdicts v JOIN session_judges.outcomes o ON o.id = v.outcome_id WHERE v.judge_model = 'google/gemini-3.5-flash-lite' AND v.rubric_version = 2 ORDER BY v.session_id, v.judged_at DESC) judged ON judged.session_id = s.id
  LEFT JOIN (SELECT t.session_id, max(t.created_at) AS last_turn_at FROM memory.operator_turns t GROUP BY t.session_id) turned ON turned.session_id = s.id
  LEFT JOIN attempts att ON att.session_id = s.id
 WHERE s.last_seen < now() - interval '60 seconds'
   AND turned.session_id IS NOT NULL
   AND (att.session_id IS NULL
        OR turned.last_turn_at > att.last_attempt_at
        OR (att.consecutive_failures > 0
            AND att.last_attempt_at + least(interval '5 minutes' * power(2, least(att.consecutive_failures, 10) - 1), interval '24 hours') < now()))
   AND (judged.session_id IS NULL
        OR (judged.is_unknown AND turned.last_turn_at > judged.judged_at)
        OR s.overview IS NULL)
 ORDER BY s.last_seen DESC
 LIMIT 25
