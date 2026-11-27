-- reads_outpace_writes found the tables a statement reads by joining every busy statement with
-- every relation outside pg_ and information_schema and matching a regex built from that
-- relation's name (b.query ~* r.pattern). Each pairing compiled and ran its own case-insensitive
-- regex over the whole statement text, so one pass ran busy x relations regexes and cost grew with
-- both; on the capture database the check averaged about 1.5 s a call on every 5-minute pass
-- (Wright problem 83252, finding 47).
--
-- wright.statement_relations reads the names after FROM and JOIN out of a statement with one regex
-- and looks them up in pg_class by name, keeping the old rules: an unqualified name is a table of
-- schema public, a qualified one names its schema, only tables, partitioned tables, views and
-- materialized views count, and pg_ and information_schema relations are left out. The check reads
-- it once per busy statement and no longer builds a pattern per relation.

SET LOCAL lock_timeout = '10s';

CREATE OR REPLACE FUNCTION wright.statement_relations(statement_text text)
RETURNS TABLE (relid oid)
LANGUAGE sql STABLE PARALLEL SAFE AS $fn$
    SELECT DISTINCT c.oid
      FROM regexp_matches(statement_text,
               '\m(?:from|join)\s+(?:only\s+)?(?:"?([a-z_][a-z0-9_$]*)"?\.)?"?([a-z_][a-z0-9_$]*)',
               'gi') AS m
      JOIN pg_namespace n ON n.nspname = lower(coalesce(m[1], 'public'))
      JOIN pg_class c ON c.relnamespace = n.oid AND c.relname = lower(m[2])
     WHERE c.relkind IN ('r', 'p', 'v', 'm')
       AND n.nspname <> 'information_schema'
       AND n.nspname !~ '^pg_'
$fn$;

UPDATE wright.checks
   SET query = replace(
                   replace(query,
                           $old$rels AS MATERIALIZED (
    SELECT c.oid AS relid,
           '\m(from|join)\s+(only\s+)?'
               || CASE WHEN n.nspname = 'public' THEN '("?public"?\.)?'
                       ELSE '"?' || n.nspname || '"?\.' END
               || '"?' || c.relname || '\M' AS pattern
      FROM pg_class c
      JOIN pg_namespace n ON n.oid = c.relnamespace
     WHERE c.relkind IN ('r', 'p', 'v', 'm')
       AND n.nspname <> 'information_schema'
       AND n.nspname !~ '^pg_'
),
$old$,
                           ''),
                   $old$    SELECT b.queryid, r.relid FROM busy b JOIN rels r ON b.query ~* r.pattern$old$,
                   $new$    SELECT b.queryid, r.relid FROM busy b CROSS JOIN LATERAL wright.statement_relations(b.query) r$new$),
       updated_at = now()
 WHERE name = 'reads_outpace_writes';

DO $check$
BEGIN
    IF EXISTS (SELECT 1 FROM wright.checks
                WHERE name = 'reads_outpace_writes'
                  AND (query LIKE '%r.pattern%'
                       OR query NOT LIKE '%wright.statement_relations(b.query)%')) THEN
        RAISE EXCEPTION 'reads_outpace_writes still matches a regex per relation';
    END IF;
END
$check$;
