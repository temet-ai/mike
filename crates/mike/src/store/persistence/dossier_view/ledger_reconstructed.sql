SELECT id AS "id!", session_id AS "session_id!", operator_turn_id AS "operator_turn_id?",
       kind AS "kind!", data AS "data!",
       CASE WHEN closed_at <= $2 THEN status ELSE 'open' END AS "status!",
       source AS "source!", source_ref AS "source_ref!", dedup_key AS "dedup_key!",
       extractor_version AS "extractor_version!", effective_at AS "effective_at!",
       recorded_at AS "recorded_at!",
       CASE WHEN closed_at <= $2 THEN closed_at END AS "closed_at?",
       CASE WHEN closed_at <= $2 THEN close_recorded_at END AS "close_recorded_at?",
       CASE WHEN closed_at <= $2 THEN closed_by END AS "closed_by?"
  FROM memory.dossier_entries
 WHERE session_id = $1
   AND source = 'capture'
   AND effective_at <= $2
   AND extractor_version = ANY($3)
UNION ALL
SELECT id, session_id, operator_turn_id, kind, data,
       CASE WHEN close_recorded_at <= $2 THEN status ELSE 'open' END,
       source, source_ref, dedup_key, extractor_version, effective_at, recorded_at,
       CASE WHEN close_recorded_at <= $2 THEN closed_at END,
       CASE WHEN close_recorded_at <= $2 THEN close_recorded_at END,
       CASE WHEN close_recorded_at <= $2 THEN closed_by END
  FROM memory.dossier_entries
 WHERE session_id = $1
   AND source <> 'capture'
   AND recorded_at <= $2
 ORDER BY "effective_at!", "id!"
