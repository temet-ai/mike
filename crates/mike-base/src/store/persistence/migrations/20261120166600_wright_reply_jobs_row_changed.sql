-- `proxy.row_changed` for new `wright.reply_jobs` rows, so Wright's
-- conversation worker on any machine wakes when the operator posts a message
-- instead of polling the queue every 5 s. The worker holds a lease
-- (`wright_conversation`) and the operator may post through another
-- machine's inspect API, so an in-process wake alone is not enough.
--
-- Only INSERT notifies: retries and lease expiries are times the worker
-- already knows from the rows it read (`next_due.sql`). Jobs are created
-- once per operator message, so the commit-time NOTIFY lock is negligible.
-- `session_id` is NULL: the conversation is not session-scoped.

CREATE OR REPLACE FUNCTION mike.notify_row_changed_wright_reply_jobs()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'wright_reply_jobs',
        'id',         NEW.id::text,
        'session_id', NULL,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE OR REPLACE TRIGGER trg_wright_reply_jobs_row_changed
AFTER INSERT ON wright.reply_jobs
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_wright_reply_jobs();
