-- Wright's checks over pg_stat_statements (slow_queries, reads_outpace_writes, idle_polls) cut the
-- statement text: the resource kept the first 120 characters of the whitespace-collapsed text, the
-- evidence field query the first 160 (slow_queries kept the first 4000 of the raw text), and
-- slow_queries compared the first 120 characters to find a statement that came back under a new
-- queryid. The operator ruled on 2026-11-24 that the whole statement is captured. This migration
-- takes every left(<text>, n) off the statement text in those stored queries; the resource keeps
-- its 'query <queryid>: ' prefix.
--
-- A problem is identified by UNIQUE (check_name, scope, resource) (open_findings.sql matches a fresh
-- finding to its problem with ON CONFLICT on it, heal_missing.sql heals every open problem the pass
-- did not report again, and slow_queries reads a statement's earlier counters from the problem with
-- the same resource). Changing the resource text alone would heal every open query problem and open
-- a new one with no history. So the resource of each query problem is rewritten here to the text the
-- new check query produces, looked up in pg_stat_statements by the queryid in its prefix. A problem
-- whose queryid pg_stat_statements no longer holds, or whose old text is not the start of the current
-- text, is left as it is. The text of triage's cause key (the first 60 characters of the resource with
-- numbers replaced) is the same before and after, so causes keep their problems.

SET LOCAL lock_timeout = '10s';

UPDATE wright.checks
   SET query = regexp_replace(
                   regexp_replace(
                       query,
                       $re$left\((regexp_replace\(btrim\(\w+\.query\), '\\s\+', ' ', 'g'\)), (?:120|160)\)$re$,
                       '\1',
                       'g'),
                   $re$left\((\w+\.query), 4000\)$re$,
                   '\1',
                   'g'),
       updated_at = now()
 WHERE query ~ $re$left\(regexp_replace\(btrim\(\w+\.query\)|left\(\w+\.query, 4000\)$re$;

DO $check$
BEGIN
    IF EXISTS (SELECT 1
                 FROM wright.checks
                WHERE query ~ $re$left\(regexp_replace\(btrim\(\w+\.query\)|left\(\w+\.query, 4000\)$re$) THEN
        RAISE EXCEPTION 'a Wright check still cuts the statement text: %',
            (SELECT string_agg(name, ', ') FROM wright.checks
              WHERE query ~ $re$left\(regexp_replace\(btrim\(\w+\.query\)|left\(\w+\.query, 4000\)$re$);
    END IF;
END
$check$;

DO $rewrite$
BEGIN
    IF to_regclass('pg_stat_statements') IS NULL THEN
        RETURN;
    END IF;
    WITH current_text AS (
        SELECT DISTINCT ON (s.queryid)
               s.queryid::text AS queryid,
               regexp_replace(btrim(s.query), '\s+', ' ', 'g') AS text
          FROM pg_stat_statements s
         WHERE s.dbid = (SELECT oid FROM pg_database WHERE datname = current_database())
           AND s.toplevel
           AND s.queryid IS NOT NULL
         ORDER BY s.queryid, s.calls DESC
    ),
    query_problems AS (
        SELECT p.id, m[1] AS queryid, m[2] AS old_text
          FROM wright.problems p
         CROSS JOIN LATERAL regexp_match(p.resource, '^query (-?[0-9]+): (.*)$', 's') AS m
         WHERE p.scope = 'query'
           AND p.check_name IN (SELECT c.name FROM wright.checks c
                                 WHERE c.query ~ $re$'query ' \|\| \w+\.queryid \|\| ': '$re$)
    )
    UPDATE wright.problems p
       SET resource = 'query ' || o.queryid || ': ' || t.text
      FROM query_problems o
      JOIN current_text t ON t.queryid = o.queryid
     WHERE p.id = o.id
       AND length(t.text) > length(o.old_text)
       AND starts_with(t.text, o.old_text);
END
$rewrite$;
