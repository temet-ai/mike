ALTER TABLE mailbox.messages
    ADD COLUMN IF NOT EXISTS held_until_turn boolean NOT NULL DEFAULT false;

COMMENT ON COLUMN mailbox.messages.held_until_turn IS 'True when the fleet wake check judged that the message does not need to wake its seat. The channel claim, the silent-seat wake and redelivery skip a held row; the drain delivers it with the seat''s next turn. Turning the decisions layer or the wake check off releases held rows at once.';

ALTER TABLE launch.tree_checks
    ADD COLUMN IF NOT EXISTS triage text;

COMMENT ON COLUMN launch.tree_checks.triage IS 'One line saying what a failed check needs: retry, fix the code or fix the environment, with the reason. Computed once when the check completes (rules first, then one Jev choice over the output tail) and printed in the receipt the seat reads. NULL for a check that passed or has not finished.';

CREATE TABLE IF NOT EXISTS launch.request_efforts (
    request_id   bigint PRIMARY KEY REFERENCES requests.requests (id) ON DELETE CASCADE,
    run_id       text        NOT NULL,
    seat         text        NOT NULL,
    session_id   bigint      NOT NULL,
    trigger      text        NOT NULL,
    label        text,
    decided_by   text,
    record_id    bigint,
    created_at   timestamptz NOT NULL DEFAULT now(),
    labelled_at  timestamptz
);

COMMENT ON TABLE launch.request_efforts IS 'One row per request a fleet seat made. `trigger` is what started the turn (operator, hook, mail, wakeup, tool_result, steering), computed by code from the request as it arrives. `label` is work, machinery or waiting: code sets it where the trigger decides it (decided_by = trigger), Jev labels the rest in paced batches (decided_by = jev, record_id = decisions.records.id). The run record sums shares by tokens from these rows.';

CREATE INDEX IF NOT EXISTS idx_request_efforts_run_seat ON launch.request_efforts (run_id, seat);

CREATE INDEX IF NOT EXISTS idx_request_efforts_unlabelled ON launch.request_efforts (run_id, request_id) WHERE label IS NULL;
