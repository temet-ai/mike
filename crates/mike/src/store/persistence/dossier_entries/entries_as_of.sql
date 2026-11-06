SELECT id, session_id, operator_turn_id, kind, data,
       CASE WHEN close_recorded_at <= $2 THEN status ELSE 'open' END AS "status!",
       source, source_ref, dedup_key, extractor_version, effective_at, recorded_at,
       CASE WHEN close_recorded_at <= $2 THEN closed_at END AS "closed_at?",
       CASE WHEN close_recorded_at <= $2 THEN close_recorded_at END AS "close_recorded_at?",
       CASE WHEN close_recorded_at <= $2 THEN closed_by END AS "closed_by?"
  FROM memory.dossier_entries
 WHERE session_id = $1
   AND recorded_at <= $2
 ORDER BY effective_at, id
