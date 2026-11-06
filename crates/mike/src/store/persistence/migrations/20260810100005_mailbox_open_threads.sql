-- Open threads (AC17): a session can start a thread with no addressed
-- recipient — a post other sessions discover and contribute to via the
-- thread transcript, rather than mail injected into one recipient's turn.
-- `to_session_id` has been NOT NULL since the mailbox's original design
-- (20260630000002); this widens it so `mailbox.messages` can hold both
-- delivery rows (addressed, injected into a turn) and post rows
-- (unaddressed, visible only via the thread transcript). Every
-- delivery-pipeline query (claim/deliver/redeliver/reap) is guarded against
-- NULL to_session_id in mailbox.rs — see
-- PersistHandle::dead_letter_expired_undelivered's `to_session_id IS NOT
-- NULL` predicate.
ALTER TABLE mailbox.messages ALTER COLUMN to_session_id DROP NOT NULL;

COMMENT ON COLUMN mailbox.messages.to_session_id IS
    'Recipient session, when this row is a delivery. NULL means this row is an open-thread post — visible in the thread transcript, never claimed, delivered, redelivered, or reaped by the mailbox drain or the TTL reaper.';
