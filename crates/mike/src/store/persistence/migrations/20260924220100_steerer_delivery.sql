--
-- Why. #355: every steerer note is pushed and nothing records how it was delivered or to whom.
-- J4 delivers efficiency steerers by append only and addresses a note about an agent's action to the
-- parent "for the agent you dispatched" (mike has no channel into an agent's lane). These two columns
-- record that, so AC-JC14-14 and AC-JC14-17 can be read and the non-actor rule can be applied.
--
-- delivery_channel has the same name and CHECK as JC17's draft (migrations/JC17_steer_question_promotion.sql
-- section 4), which adds it with ADD COLUMN IF NOT EXISTS and re-creates the CHECK with the same
-- values, so either landing order is safe. If JC17 lands first, this file's delivery_channel lines are
-- no-ops. Standing decision 8 applies to both CHECKs: regenerate from origin/main at build time.
--
-- Expand-only. steering.interventions is written continuously by capture, so this file sets its own
-- short lock_timeout, as JC06's does: it fails fast and is retried rather than stalling capture.

SET LOCAL lock_timeout = '5s';

ALTER TABLE steering.interventions
    ADD COLUMN IF NOT EXISTS delivery_channel text,
    ADD COLUMN IF NOT EXISTS addressed_to     text;

ALTER TABLE steering.interventions DROP CONSTRAINT IF EXISTS interventions_delivery_channel_check;
ALTER TABLE steering.interventions
    ADD CONSTRAINT interventions_delivery_channel_check
        CHECK (delivery_channel IS NULL OR delivery_channel IN ('append', 'wake'));

ALTER TABLE steering.interventions DROP CONSTRAINT IF EXISTS interventions_addressed_to_check;
ALTER TABLE steering.interventions
    ADD CONSTRAINT interventions_addressed_to_check
        CHECK (addressed_to IS NULL OR addressed_to IN ('session', 'parent_for_actor'));

COMMENT ON COLUMN steering.interventions.delivery_channel IS
    'How the note was delivered: append (queued for the session''s next request, never pushed) or wake (JC17: queued, and pushed on the quiescence tick if still undelivered). NULL = pushed at once, the behaviour before JC14 J4.';
COMMENT ON COLUMN steering.interventions.addressed_to IS
    'session = the note is about the session''s own lane; parent_for_actor = the trigger request was an agent''s, and the note was addressed to the parent as guidance for the agent it dispatched (JC14 §4.6 item 7). NULL = written before JC14 J4.';
