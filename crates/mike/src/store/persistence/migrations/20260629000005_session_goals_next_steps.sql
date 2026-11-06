-- Add next_steps column to sessions.session_goals.
-- Idempotent: IF NOT EXISTS guards against double-apply.
ALTER TABLE sessions.session_goals
    ADD COLUMN IF NOT EXISTS next_steps text NOT NULL DEFAULT '';
