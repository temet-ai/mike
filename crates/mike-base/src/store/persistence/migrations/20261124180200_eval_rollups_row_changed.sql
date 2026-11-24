-- `proxy.row_changed` for eval.rollups. The cross-session worker read the newest finished
-- calibration rollup and its rates every minute on every machine (about 280 calls an hour)
-- while a rollup finishes at most a few times a day (Wright finding 34, problem 73851). It now
-- reads them again only after a rollup row finished, was added or was removed. Rates are written
-- in the same transaction that sets finished_at, so a rollup's notice covers its rates.

CREATE OR REPLACE FUNCTION mike.notify_row_changed_eval_rollups()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'eval_rollups',
        'id',         COALESCE(NEW.id, OLD.id)::text,
        'session_id', NULL,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE OR REPLACE TRIGGER trg_eval_rollups_row_changed
AFTER INSERT OR DELETE ON eval.rollups
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_eval_rollups();

CREATE OR REPLACE TRIGGER trg_eval_rollups_row_changed_update
AFTER UPDATE ON eval.rollups
FOR EACH ROW WHEN (OLD.finished_at IS DISTINCT FROM NEW.finished_at)
EXECUTE FUNCTION mike.notify_row_changed_eval_rollups();
