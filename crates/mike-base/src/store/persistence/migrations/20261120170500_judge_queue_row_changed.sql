-- `proxy.row_changed` (feed table `judge_queue`) when a turn becomes claimable
-- by the live judge sweep through a write: `judge_queued_at` is set on a turn
-- that was not queued (the turn_facts upsert, the hourly queue reconcile, a
-- rejudge clearing failures), or a queued turn's failure or backoff is cleared
-- (a rejudge, or the release of transient backoff after a verdict lands).
--
-- The live judge sweep claimed on every tick (every 60 s on the machine
-- holding the judge_worker lease). On 2026-11-23 that claim (queryid
-- 55606225969031222) ran about 60 times an hour and returned nothing: the
-- immediate judge on the closing machine takes almost every turn first. The
-- sweep now claims after this notice, a listener resync or a 600 s backstop,
-- which also covers what time alone frees: an elapsed backoff and an expired
-- lease. The payload is the same for every row, so a statement that queues
-- many turns in one transaction sends one notice.

CREATE OR REPLACE FUNCTION mike.notify_row_changed_judge_queue()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'judge_queue',
        'id',         'memory.operator_turns',
        'session_id', NULL,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE OR REPLACE TRIGGER trg_operator_turns_judge_queue_row_changed
AFTER UPDATE OF judge_queued_at, judge_failure_reason, judge_next_attempt_at
ON memory.operator_turns
FOR EACH ROW
WHEN (NEW.judge_queued_at IS NOT NULL AND (
        OLD.judge_queued_at IS NULL
     OR (OLD.judge_failure_reason IS NOT NULL AND NEW.judge_failure_reason IS NULL)
     OR (OLD.judge_next_attempt_at IS NOT NULL AND NEW.judge_next_attempt_at IS NULL)))
EXECUTE FUNCTION mike.notify_row_changed_judge_queue();
