-- Wright's checks over pg_stat_statements (slow_queries, reads_outpace_writes, idle_polls) cut the
-- statement text: the resource kept the first 120 characters of the whitespace-collapsed text, the
-- evidence field query the first 160 (slow_queries kept the first 4000 of the raw text), and
-- slow_queries compared the first 120 characters to find a statement that came back under a new
-- queryid. The operator ruled on 2026-11-24 that the whole statement is captured.
--
-- A problem is identified by UNIQUE (check_name, scope, resource) and that btree index cannot hold a
-- whole statement (an entry is limited to about 8 kB), nor may a hash of the text be indexed. So the
-- resource keeps the 'query <queryid>: ' prefix and the first 120 characters, exactly as before, and
-- the whole text goes to places that are not indexed:
--
--   * the evidence field query of the three checks carries the whole statement (no left());
--   * slow_queries compares whole collapsed statements (text_key) to find one that came back under a
--     new queryid;
--   * wright.problems.statement (text, null for a check that is not about a statement, never indexed)
--     holds the whole text. open_findings.sql sets it from the finding's evidence on every pass;
--     this migration fills it for the existing query problems from pg_stat_statements, by the
--     queryid in the resource, where the statement is still there.
--
-- wright.cause_queue gains statement (the representative problem's) after its existing columns.

SET LOCAL lock_timeout = '10s';

ALTER TABLE wright.problems ADD COLUMN IF NOT EXISTS statement text;

UPDATE wright.checks
   SET query = regexp_replace(
                   regexp_replace(
                       regexp_replace(
                           query,
                           $re$('query', )left\((regexp_replace\(btrim\(\w+\.query\), '\\s\+', ' ', 'g'\)), 160\)$re$,
                           '\1\2',
                           'g'),
                       $re$('query', )left\((\w+\.query), 4000\)$re$,
                       '\1\2',
                       'g'),
                   $re$left\((regexp_replace\(btrim\(\w+\.query\), '\\s\+', ' ', 'g'\)), 120\)( AS text_key)$re$,
                   '\1\2',
                   'g'),
       updated_at = now()
 WHERE query ~ $re$'query', left\(|left\(regexp_replace\(btrim\(\w+\.query\), '\\s\+', ' ', 'g'\), 120\) AS text_key$re$;

DO $check$
BEGIN
    IF EXISTS (SELECT 1
                 FROM wright.checks
                WHERE query ~ $re$'query', left\(|left\(regexp_replace\(btrim\(\w+\.query\), '\\s\+', ' ', 'g'\), 120\) AS text_key$re$) THEN
        RAISE EXCEPTION 'a Wright check still cuts the statement text: %',
            (SELECT string_agg(name, ', ') FROM wright.checks
              WHERE query ~ $re$'query', left\(|left\(regexp_replace\(btrim\(\w+\.query\), '\\s\+', ' ', 'g'\), 120\) AS text_key$re$);
    END IF;
END
$check$;

DO $fill$
BEGIN
    IF to_regclass('pg_stat_statements') IS NULL THEN
        RETURN;
    END IF;
    WITH current_text AS (
        SELECT DISTINCT ON (s.queryid)
               s.queryid::text AS queryid,
               s.query AS raw,
               regexp_replace(btrim(s.query), '\s+', ' ', 'g') AS collapsed
          FROM pg_stat_statements s
         WHERE s.dbid = (SELECT oid FROM pg_database WHERE datname = current_database())
           AND s.toplevel
           AND s.queryid IS NOT NULL
         ORDER BY s.queryid, s.calls DESC
    ),
    query_problems AS (
        SELECT p.id, p.check_name, m[1] AS queryid, m[2] AS named_text
          FROM wright.problems p
         CROSS JOIN LATERAL regexp_match(p.resource, '^query (-?[0-9]+): (.*)$', 's') AS m
         WHERE p.scope = 'query'
           AND p.statement IS NULL
           AND p.check_name IN (SELECT c.name FROM wright.checks c
                                 WHERE c.query ~ $re$'query ' \|\| \w+\.queryid \|\| ': '$re$)
    )
    UPDATE wright.problems p
       SET statement = CASE WHEN o.check_name = 'slow_queries' THEN t.raw ELSE t.collapsed END
      FROM query_problems o
      JOIN current_text t ON t.queryid = o.queryid
     WHERE p.id = o.id
       AND starts_with(t.collapsed, o.named_text);
END
$fill$;

CREATE OR REPLACE VIEW wright.cause_queue AS
WITH open_eps AS (
    SELECT p.id AS problem_id, p.cause_id, p.check_name, p.resource, p.opened_at,
           e.id AS episode_id, e.observations
      FROM wright.problems p
      JOIN wright.episodes e ON e.problem_id = p.id AND e.closed_at IS NULL
     WHERE p.state = 'open'
       AND p.cause_id IS NOT NULL
),
found AS (
    SELECT o.cause_id, max(f.id) AS finding_id
      FROM open_eps o
      JOIN wright.findings f ON f.episode_id = o.episode_id
     GROUP BY o.cause_id
),
followed AS (
    SELECT o.cause_id, max(fu.id) AS followup_id, max(fu.issue_number) AS issue_number
      FROM open_eps o
      JOIN wright.findings f ON f.problem_id = o.problem_id
      JOIN wright.followup_findings ff ON ff.finding_id = f.id
      JOIN wright.followups fu ON fu.id = ff.followup_id
     WHERE fu.state IN ('open', 'filed', 'handed_off', 'batched', 'stuck')
     GROUP BY o.cause_id
),
tried AS (
    SELECT o.cause_id,
           count(*) FILTER (WHERE i.state <> 'unavailable') AS attempts,
           max(i.created_at) AS last_attempt_at
      FROM open_eps o
      JOIN wright.investigations i ON i.episode_id = o.episode_id
     GROUP BY o.cause_id
),
grouped AS (
    SELECT o.cause_id,
           count(*) AS problems,
           sum(o.observations) AS observations,
           min(o.opened_at) AS opened_at,
           (array_agg(o.problem_id ORDER BY o.observations DESC, o.problem_id))[1] AS problem_id,
           (array_agg(o.episode_id ORDER BY o.observations DESC, o.problem_id))[1] AS episode_id,
           (array_agg(o.check_name ORDER BY o.observations DESC, o.problem_id))[1] AS check_name,
           (array_agg(o.resource ORDER BY o.observations DESC, o.problem_id))[1] AS resource
      FROM open_eps o
     GROUP BY o.cause_id
)
SELECT g.cause_id, g.problem_id, g.episode_id, g.check_name, g.resource,
       g.problems, g.observations, g.opened_at,
       COALESCE(t.attempts, 0) AS attempts, t.last_attempt_at,
       fd.finding_id, fw.followup_id, fw.issue_number,
       wright.has_enabled_remedy(g.check_name) AS remedied,
       s.priority,
       (c.pinned_at IS NOT NULL) AS pinned,
       c.operator_priority,
       COALESCE(c.operator_priority, s.priority) AS rank_priority,
       c.group_id,
       rp.statement
  FROM grouped g
  JOIN wright.causes c ON c.id = g.cause_id
  CROSS JOIN LATERAL (
      SELECT round((g.problems + ln(1 + g.observations)
                    + extract(epoch FROM now() - g.opened_at) / 86400.0)::numeric, 2)::float8 AS priority
  ) s
  LEFT JOIN tried t USING (cause_id)
  LEFT JOIN found fd USING (cause_id)
  LEFT JOIN followed fw USING (cause_id)
  JOIN wright.problems rp ON rp.id = g.problem_id
;
