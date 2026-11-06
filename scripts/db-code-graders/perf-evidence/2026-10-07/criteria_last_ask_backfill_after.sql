UPDATE memory.acceptance_criteria_maps m
   SET last_ask_entry_id = seen.newest
  FROM (
        SELECT m2.id AS map_id, max(e.id) AS newest
          FROM memory.acceptance_criteria_maps m2
          JOIN memory.ask_ledger_entries e
            ON e.session_id = m2.session_id AND e.created_at <= m2.created_at
         WHERE m2.last_ask_entry_id IS NULL
         GROUP BY m2.id
       ) seen
 WHERE m.id = seen.map_id
