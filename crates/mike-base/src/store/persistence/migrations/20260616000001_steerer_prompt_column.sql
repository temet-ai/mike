-- Add authored prompt column to steering.steerers.
--
-- Additive only — no backfill, no NOT NULL, no data migration.
-- NULL means "use the existing agent-composition / static fallback".
-- A non-NULL, non-empty value is preferred by resolve_verdict_system_prompt
-- over the composition lookup and the static default.
--
-- Expand-only: safe to apply while the proxy is running.

ALTER TABLE steering.steerers
    ADD COLUMN IF NOT EXISTS prompt text;
