SELECT schemaname::text AS "schema!", relname::text AS "table!",
                          indexrelname::text AS "index!",
                          pg_relation_size(indexrelid) AS "bytes!",
                          idx_scan AS "scans!", stats_reset
                     FROM pg_stat_user_indexes
                    ORDER BY pg_relation_size(indexrelid) DESC, indexrelid
                    LIMIT 30
