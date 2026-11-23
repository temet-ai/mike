DELETE FROM wright.table_counters t
 WHERE t.taken_at NOT IN (SELECT c.taken_at FROM wright.statement_counters c)
