-- Each of mike's pool classes now connects with its own application_name
-- (claude-proxy-capture, claude-proxy-interactive, claude-proxy-background), so
-- pg_stat_activity and the Postgres log name the pool that held or waited for a connection
-- (Wright finding 36). A write from any of them is still mike's own in the config change log.
CREATE OR REPLACE FUNCTION config.default_change_via() RETURNS text
LANGUAGE sql STABLE AS $$
    SELECT CASE
               WHEN current_setting('application_name', true) = 'claude-proxy-migrate' THEN 'migration'
               WHEN current_setting('application_name', true) IN
                    ('claude-proxy', 'claude-proxy-capture', 'claude-proxy-interactive',
                     'claude-proxy-background') THEN 'mike'
               ELSE 'sql'
           END
$$;
