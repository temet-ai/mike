-- `proxy.row_changed` (feed table `correction_qualifications`) when a turn
-- becomes a qualified correction: a `corrected` reaction label in
-- `eval.labels`, or a live, projected `corrected` verdict in
-- `memory.turn_verdicts`. The semantic worker's correction tagger waits for it
-- before it asks again for corrections without a pattern assignment.
--
-- The tagger asked on every tick (every 300 s on each machine). On 2026-11-22
-- that statement (queryid -2995925360253255084) ran about 70 times an hour
-- across the five machines and returned nothing. A correction can become
-- taggable only when its turn_prompt item is written (the same worker's build
-- step, which the worker sees itself), or when the turn before it is qualified
-- as corrected, which is what this notice reports. Verdicts are append-only
-- (the projected one is inserted with `projected` set), so inserts are
-- enough. Live corrected verdicts arrive a few times an hour; label inserts
-- notify once per statement, so a bulk label import takes the commit-time
-- NOTIFY lock once.

CREATE OR REPLACE FUNCTION mike.notify_row_changed_correction_labels()
RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    newest bigint;
BEGIN
    SELECT max(l.id) INTO newest
      FROM new_labels l
     WHERE l.subject_kind = 'turn' AND l.axis = 'reaction' AND l.label = 'corrected';
    IF newest IS NOT NULL THEN
        PERFORM pg_notify('proxy.row_changed', json_build_object(
            'table',      'correction_qualifications',
            'id',         'eval.labels:' || newest,
            'session_id', NULL,
            'op',         TG_OP
        )::text);
    END IF;
    RETURN NULL;
END; $$;

CREATE OR REPLACE TRIGGER trg_eval_labels_correction_qualifications_row_changed
AFTER INSERT ON eval.labels
REFERENCING NEW TABLE AS new_labels
FOR EACH STATEMENT
EXECUTE FUNCTION mike.notify_row_changed_correction_labels();

CREATE OR REPLACE FUNCTION mike.notify_row_changed_correction_verdicts()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'correction_qualifications',
        'id',         'memory.turn_verdicts:' || NEW.id,
        'session_id', NULL,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE OR REPLACE TRIGGER trg_turn_verdicts_correction_qualifications_row_changed
AFTER INSERT ON memory.turn_verdicts
FOR EACH ROW
WHEN (NEW.mode = 'live' AND NEW.projected AND NEW.reaction = 'corrected')
EXECUTE FUNCTION mike.notify_row_changed_correction_verdicts();
