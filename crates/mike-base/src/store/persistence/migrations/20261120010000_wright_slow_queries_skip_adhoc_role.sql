-- slow_queries read every toplevel statement in pg_stat_statements, and mike, every session's
-- ad-hoc psql and every benchmark replay all connect as the same role, so a hand-written
-- analysis query or a benchmark with its values inlined (queryid -4543664411372524603, problems
-- 51063 and 58600) opened a problem for a statement mike never runs and kept it open for
-- quiet_hours. Ad-hoc access now has its own read-only role, mike_adhoc (AGENTS.md's read-only
-- SQL line connects as it; `SET ROLE mike_adhoc` works too), and the check skips statements run
-- by a role listed in its skip_roles param. pg_stat_statements records the executing role, so
-- this needs no change to how mike itself connects.
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'mike_adhoc') THEN
        CREATE ROLE mike_adhoc LOGIN;
    END IF;
    GRANT pg_read_all_data, pg_read_all_stats TO mike_adhoc;
EXCEPTION
    -- Roles are cluster-wide: test databases migrating in parallel can race on the create.
    WHEN duplicate_object OR unique_violation THEN NULL;
    WHEN insufficient_privilege THEN
        RAISE NOTICE 'mike_adhoc role not created: the migrating role cannot create roles';
END
$$;

UPDATE wright.checks
   SET query = replace(query,
                       $old$       AND s.query !~* '^\s*prepare\s'$old$,
                       $new$       AND s.query !~* '^\s*prepare\s'
       AND s.userid NOT IN (SELECT r.oid FROM pg_roles r
                             WHERE r.rolname IN (SELECT jsonb_array_elements_text(
                                                     coalesce($1->'skip_roles', '[]'::jsonb))))$new$),
       params = params || '{"skip_roles": ["mike_adhoc"]}'::jsonb,
       expects = expects || '; statements run by a skip_roles role (ad-hoc psql, benchmarks) are not reported',
       updated_at = now()
 WHERE name = 'slow_queries'
   AND query NOT LIKE '%skip_roles%';
