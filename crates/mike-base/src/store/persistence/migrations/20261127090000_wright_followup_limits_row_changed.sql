-- `proxy.row_changed` for wright.followup_limits, so its readers keep the row
-- until the operator changes it instead of reading it on every worker tick.
--
-- The follow-ups worker (every 60 s, before its lease check), the repair
-- worker's fix-host check (every 2 min, no lease) and the engineer (every
-- 60 s) each read this one-row table in every serve process, about 411 reads
-- an hour with no writes. The row changes only through the operator's limits
-- patch, a few times a month, so the NOTIFY cost is negligible.

CREATE OR REPLACE FUNCTION mike.notify_row_changed_wright_followup_limits()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'wright_followup_limits',
        'id',         NEW.id::text,
        'session_id', NULL,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE OR REPLACE TRIGGER trg_wright_followup_limits_row_changed
AFTER INSERT ON wright.followup_limits
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_wright_followup_limits();

CREATE OR REPLACE TRIGGER trg_wright_followup_limits_row_changed_update
AFTER UPDATE ON wright.followup_limits
FOR EACH ROW WHEN (OLD.* IS DISTINCT FROM NEW.*)
EXECUTE FUNCTION mike.notify_row_changed_wright_followup_limits();
