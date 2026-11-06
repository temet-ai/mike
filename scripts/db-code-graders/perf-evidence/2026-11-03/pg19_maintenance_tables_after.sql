SELECT t.schemaname::text AS "schema!", t.relname::text AS "table!",
                          t.n_dead_tup AS "dead_tuples!",
                          t.total_autovacuum_time AS "total_autovacuum_ms!", t.stats_reset,
                          a.score AS "autovacuum_score?", a.do_vacuum AS "do_vacuum?",
                          a.do_analyze AS "do_analyze?", a.for_wraparound AS "for_wraparound?"
                     FROM pg_stat_user_tables t
                     LEFT JOIN pg_stat_autovacuum_scores a ON a.relid = t.relid
                    ORDER BY t.n_dead_tup DESC, t.relid
                    LIMIT 50
