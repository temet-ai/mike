-- Generalize the push-wake path (issue #255) from mailbox-only to every
-- queued `SystemReminder` source. `remote_control.injections` recorded only
-- mailbox provenance (`message_uuid`) because `push_mailbox` was the only
-- producer; `push_reminder` now accepts every `ReminderSource`, and every
-- injection row needs to say which one produced it.
--
-- `source_kind` mirrors `store::models::ReminderSource`'s variant names
-- lowercased: 'mailbox', 'steerer', 'lesson', 'diagnostic', 'recall'.
-- Defaulted to 'mailbox' and backfilled from the existing `message_uuid`
-- column so every already-captured row keeps honest provenance without a
-- second pass. `source_id` is the source's own natural identifier as text
-- (the mailbox message uuid, the steering intervention uuid, or NULL for
-- lesson/diagnostic/recall — see the design note below) — text rather than
-- uuid because a future source may not have one shaped like a uuid, and the
-- column is never joined on, only read back for display and ack routing.
-- `message_uuid` stays as its own column (not superseded by `source_id`):
-- it carries the FK to `mailbox.messages`, which `source_id` deliberately
-- does not (a text column can't reference a uuid PK, and the mailbox claim
-- path already depends on the FK's ON DELETE SET NULL behaviour).
ALTER TABLE remote_control.injections
    ADD COLUMN IF NOT EXISTS source_kind text NOT NULL DEFAULT 'mailbox',
    ADD COLUMN IF NOT EXISTS source_id text;

UPDATE remote_control.injections
   SET source_id = message_uuid::text
 WHERE message_uuid IS NOT NULL AND source_id IS NULL;

CREATE INDEX IF NOT EXISTS idx_rc_injections_source
    ON remote_control.injections (source_kind, source_id)
    WHERE source_id IS NOT NULL;

COMMENT ON COLUMN remote_control.injections.source_kind IS
    'Which ReminderSource produced this push attempt: mailbox, steerer, '
    'lesson, diagnostic, or recall. Defaults to mailbox for rows captured '
    'before this column existed.';
COMMENT ON COLUMN remote_control.injections.source_id IS
    'The source''s own identifier as text (mailbox message_uuid, steering '
    'intervention uuid), or NULL for a source with no owning row to claim '
    '(lesson, diagnostic, recall) or a pre-existing mailbox row this '
    'migration could not backfill.';

-- `steering.interventions` gains its own push-claim/ack pair, mirroring
-- `mailbox.messages`' claimed_at/consumed_at sentinel: `pushed_at` is the
-- claim (stamped atomically with the injection insert, so a session cannot
-- be pushed the same intervention twice), `acked_at` is the client's
-- confirmed receipt. Both nullable and independent of the existing
-- sent_at/processed_at/delivered_request_id trio, which record the
-- request-time delivery protocol (payload appended to an outgoing body) —
-- a pushed intervention may carry both a push record and, later, a
-- request-time record if the push was never acked and the reminder fell
-- back to the request-time drain.
ALTER TABLE steering.interventions
    ADD COLUMN IF NOT EXISTS pushed_at timestamptz,
    ADD COLUMN IF NOT EXISTS acked_at timestamptz;

CREATE INDEX IF NOT EXISTS idx_interventions_pushed_unacked
    ON steering.interventions (session_id)
    WHERE pushed_at IS NOT NULL AND acked_at IS NULL;

COMMENT ON COLUMN steering.interventions.pushed_at IS
    'Stamped atomically with the remote_control.injections insert when this '
    'intervention was pushed into the recipient''s live Remote Control '
    'stream rather than queued for request-time delivery. Cleared back to '
    'NULL if the push is never acked past its grace window, or if the '
    'emission attempt itself failed, so the request-time drain treats it '
    'as an ordinary pending reminder again.';
COMMENT ON COLUMN steering.interventions.acked_at IS
    'Stamped when the pushed client confirmed receipt (POST '
    '/worker/events/delivery), matched through the owning '
    'remote_control.injections row''s source_id. NULL for an intervention '
    'that was never pushed, or pushed but not yet (or never) acked.';
