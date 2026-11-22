-- `proxy.row_changed` for `steering.steerers`, so readers of a steerer row can keep it in
-- memory until it changes.
--
-- `guard_steerer` read the turn-guard row on every cross-session tick (each minute on every
-- machine), every label pass and every demotion check: about 311 reads an hour summed across
-- machines on 2026-11-22, each returning the same row. It now answers from memory until this
-- notice arrives. Steerers change a few times a day, so the commit-time NOTIFY lock is
-- negligible. `session_id` is NULL: a steerer is not session-scoped.

CREATE OR REPLACE FUNCTION mike.notify_row_changed_steerers()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'steerers',
        'id',         COALESCE(NEW.id, OLD.id)::text,
        'session_id', NULL,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE OR REPLACE TRIGGER trg_steerers_row_changed
AFTER INSERT OR DELETE ON steering.steerers
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_steerers();

CREATE OR REPLACE TRIGGER trg_steerers_row_changed_update
AFTER UPDATE ON steering.steerers
FOR EACH ROW WHEN (OLD.* IS DISTINCT FROM NEW.*)
EXECUTE FUNCTION mike.notify_row_changed_steerers();
