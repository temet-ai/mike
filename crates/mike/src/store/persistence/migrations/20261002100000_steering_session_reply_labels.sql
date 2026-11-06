-- A delivered steering note is labelled from the session's reply to it: a judge reads the note,
-- what triggered it and the reply, and says whether the note was right. That source is
-- session_reply, and it is valid only on a row whose note was delivered.
ALTER TABLE steering.evaluations DROP CONSTRAINT IF EXISTS evaluations_label_source_check;
ALTER TABLE steering.evaluations
    ADD CONSTRAINT evaluations_label_source_check
        CHECK (label_source IS NULL
               OR label_source IN ('next_message', 'session_end', 'operator', 'replay', 'session_reply'));

ALTER TABLE steering.evaluations DROP CONSTRAINT IF EXISTS evaluations_session_reply_needs_delivery;
ALTER TABLE steering.evaluations
    ADD CONSTRAINT evaluations_session_reply_needs_delivery
        CHECK (label_source IS DISTINCT FROM 'session_reply' OR intervention_uuid IS NOT NULL);

-- Replaces 20260925000700's constraint of the same name. Automatic labels from the next message
-- or the session end stay invalid on a row whose note reached the session. A live row that
-- passed without firing delivered nothing, exactly like a shadow or held-back row, so it may
-- carry one; the labeller has written such labels since live passes were admitted on 2026-09-29,
-- and this constraint rejected every batch that held one.
ALTER TABLE steering.evaluations DROP CONSTRAINT IF EXISTS evaluations_auto_label_shadow_only;
ALTER TABLE steering.evaluations
    ADD CONSTRAINT evaluations_auto_label_shadow_only
        CHECK (label_source IS NULL
               OR label_source NOT IN ('next_message', 'session_end')
               OR mode = 'shadow'
               OR arm IS NOT DISTINCT FROM 'holdback'
               OR intervention_uuid IS NULL);

-- The reply labeller's queue: fired rows still unlabelled.
CREATE INDEX IF NOT EXISTS idx_evaluations_reply_label_queue
    ON steering.evaluations (dispatched_at)
    WHERE intervention_uuid IS NOT NULL AND outcome_label IS NULL;
