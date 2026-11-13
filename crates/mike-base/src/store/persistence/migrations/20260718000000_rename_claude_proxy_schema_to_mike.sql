-- Renames the read-API schema from claude_proxy to mike, carrying its 9
-- views and 5 notify_row_changed_* trigger functions forward intact. The
-- domain schemas (requests, sessions, tools, etc.) that own the real
-- tables are untouched. Idempotent: a rerun after the schema is already
-- named `mike` is a no-op.
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_namespace WHERE nspname = 'claude_proxy') THEN
        ALTER SCHEMA claude_proxy RENAME TO mike;
    END IF;
END
$$;
