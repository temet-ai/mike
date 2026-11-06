SELECT ord, kind, entry_date, session_uuid, commit_sha, summary, body, data, created_at
  FROM work.thread_log
 WHERE thread_id = 7 AND (NULL::int IS NULL OR ord < NULL::int)
 ORDER BY ord DESC
 LIMIT 21;
