SELECT backend_type AS "backend_type!", object AS "object!",
                          context AS "context!", read_bytes::bigint, stats_reset
                     FROM pg_stat_io ORDER BY backend_type, object, context
