WITH per_entry AS (
    SELECT DISTINCT ON (e.id) e.id AS entry_id, r.id AS request_id
      FROM memory.ask_ledger_entries e
      JOIN requests.requests r ON r.session_id = e.session_id
      JOIN requests.turn_origins o ON o.id = r.turn_origin_id
                                 AND o.code IN ('primary', 'primary_first')
      JOIN requests.turn_types tt ON tt.id = r.turn_type_id AND tt.code = 'user_prompt'
     WHERE e.request_id IS NULL
       AND e.session_id = ANY($1)
       AND r.created_at BETWEEN e.created_at - interval '10 minutes'
                            AND e.created_at + interval '5 seconds'
       AND btrim(r.human_prompt) = e.content
       AND NOT EXISTS (SELECT 1 FROM memory.ask_ledger_entries x WHERE x.request_id = r.id)
     ORDER BY e.id, r.created_at DESC
), matched AS (
    SELECT DISTINCT ON (request_id) entry_id, request_id
      FROM per_entry
     ORDER BY request_id, entry_id
)
SELECT count(*) AS "matched!" FROM matched
