SELECT s.id AS session_db_id,
       s.session_uuid::text AS session_id
  FROM sessions.sessions s
 WHERE s.last_seen < (now() - interval '60 seconds')
   AND EXISTS (
       SELECT 1 FROM requests.requests r WHERE r.session_id = s.id
   )
   AND NOT EXISTS (
       SELECT 1 FROM session_judges.verdicts v
        WHERE v.session_id = s.id
          AND v.judge_model = 'google/gemini-2.5-flash-lite'
          AND v.rubric_version = 1
   )
 ORDER BY s.last_seen DESC
 LIMIT 25
