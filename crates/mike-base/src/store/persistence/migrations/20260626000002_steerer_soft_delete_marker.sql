-- Soft-delete marker for steerers — a deleted status distinct from `enabled`.
--
-- `enabled = false` is the user-facing disable toggle: the steerer stays in the
-- fleet roster and can be re-enabled. It is NOT a delete. Deleting a steerer
-- needs its own terminal state, otherwise "delete" and "disable" write the same
-- row and the deleted steerer keeps showing up in the roster (just toggled off).
--
-- `deleted_at` is that terminal state: a deleted steerer is dropped from the
-- roster (list_steerers filters `deleted_at IS NULL`) while its row and all its
-- intervention attribution history are retained — interventions.steerer_id is
-- ON DELETE RESTRICT, so the row must survive. NULL = live, non-NULL = deleted.

ALTER TABLE steering.steerers
    ADD COLUMN IF NOT EXISTS deleted_at timestamptz;
