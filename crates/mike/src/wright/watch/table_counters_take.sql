INSERT INTO wright.table_counters (taken_at, relid, writes)
SELECT now(), t.relid, t.n_tup_ins + t.n_tup_upd + t.n_tup_del
  FROM pg_stat_user_tables t
 WHERE EXISTS (SELECT 1 FROM wright.statement_counters c WHERE c.taken_at = now())
ON CONFLICT (taken_at, relid) DO NOTHING
