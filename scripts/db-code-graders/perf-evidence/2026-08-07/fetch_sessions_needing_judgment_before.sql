SELECT s.id AS session_db_id,
       s.session_uuid::text AS session_id,
       (judged.session_id IS NULL) AS needs_verdict
  FROM sessions.sessions s
  LEFT JOIN (
       SELECT DISTINCT v.session_id
         FROM session_judges.verdicts v
        WHERE v.judge_model = 'google/gemini-2.5-flash-lite'
          AND v.rubric_version = 1
  ) judged ON judged.session_id = s.id
  LEFT JOIN (
       SELECT DISTINCT t.session_id
         FROM memory.operator_turns t
  ) turned ON turned.session_id = s.id
 WHERE s.last_seen < now() - interval '60 seconds'
   AND EXISTS (
       SELECT 1
         FROM requests.requests r
        WHERE r.session_id = s.id
   )
   AND (
       judged.session_id IS NULL
       OR (s.overview IS NULL AND turned.session_id IS NOT NULL)
   )
 ORDER BY s.last_seen DESC
 LIMIT 25
