WITH latest AS (
    SELECT DISTINCT ON (m.session_id) m.id AS map_id, m.session_id, m.ask_entry_count
      FROM memory.acceptance_criteria_maps m
     WHERE m.session_id = ANY($1)
     ORDER BY m.session_id, m.version DESC
), genuine AS (
    SELECT e.session_id, count(*) AS n
      FROM memory.ask_ledger_entries e
      LEFT JOIN prompt_kinds.classifications c ON c.request_id = e.request_id
      LEFT JOIN prompt_kinds.kinds k ON k.id = c.kind_id
     WHERE e.session_id = ANY($1)
       AND (k.code IS NULL OR k.code = 'operator')
     GROUP BY e.session_id
), recount AS (
    SELECT l.map_id, l.session_id, l.ask_entry_count AS old_count,
           LEAST(COALESCE(g.n, 0), $2)::int AS new_count
      FROM latest l
      LEFT JOIN genuine g ON g.session_id = l.session_id
     WHERE LEAST(COALESCE(g.n, 0), $2) < l.ask_entry_count
), updated AS (
    UPDATE memory.acceptance_criteria_maps m
       SET ask_entry_count = r.new_count
      FROM recount r
     WHERE m.id = r.map_id AND r.new_count < m.ask_entry_count
 RETURNING m.id
)
SELECT r.map_id AS "map_id!", r.session_id AS "session_id!",
       r.old_count AS "old_count!", r.new_count AS "new_count!"
  FROM recount r JOIN updated u ON u.id = r.map_id
 ORDER BY r.map_id
