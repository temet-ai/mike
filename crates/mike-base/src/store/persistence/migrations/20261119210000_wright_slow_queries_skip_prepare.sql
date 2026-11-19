-- slow_queries already skips EXPLAIN statements, but scripts/db-perf-check measures a query by
-- running it as an SQL-level PREPARE ks(...) / EXECUTE, so every measurement run opened a
-- "PREPARE ks" problem of its own (five on 2026-11-19, 400-1650 ms each). mike's own queries are
-- prepared through the wire protocol and never start with PREPARE, so such statements are
-- measurement tooling and are skipped too.
UPDATE wright.checks
   SET query = replace(query,
                       $old$       AND s.query !~* '^\s*explain'$old$,
                       $new$       AND s.query !~* '^\s*explain'
       AND s.query !~* '^\s*prepare\s'$new$),
       expects = expects || '; SQL-level PREPARE statements (measurement tooling) are not reported',
       updated_at = now()
 WHERE name = 'slow_queries'
   AND query NOT LIKE '%^\s*prepare\s%';
