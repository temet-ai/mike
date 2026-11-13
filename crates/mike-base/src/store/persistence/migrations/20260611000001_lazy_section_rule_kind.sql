-- Lazy prompt sections (docs/plans/lazy-prompt-sections.md §Rule shape).
--
-- The upcoming `lazy_section` rule kind strips a heading-delimited system-prompt
-- section and replaces it with an operator-authored stub carrying a trigger
-- token (e.g. !!CHROME!!); when that token later appears in assistant text, the
-- stripped section is re-injected as a <system-reminder>. This migration adds
-- the storage half: the new kind row and the trigger_token column. Expand-only.

-- 1. New rule kind.
--    There is no stringly-typed `kind` column on rules.rules and therefore no
--    CHECK constraint to extend: kind is normalised as
--    `kind_id bigint NOT NULL REFERENCES rules.kinds(id)` (lookup table, see
--    20260604000001_baseline.sql). Admitting 'lazy_section' = inserting its row.
--    Explicit id 3 matches the baseline's deterministic seeding of ids 1..2.
INSERT INTO rules.kinds (id, code) OVERRIDING SYSTEM VALUE
VALUES (3, 'lazy_section')
ON CONFLICT DO NOTHING;

-- The baseline seeded ids 1..2 with OVERRIDING SYSTEM VALUE but never advanced
-- the identity sequence (live dev DB: last_value=1, is_called=false), so any
-- future plain INSERT into rules.kinds would collide on id=1. Re-sync the
-- sequence past the highest explicitly-assigned id (idempotent).
SELECT setval(
    pg_get_serial_sequence('rules.kinds', 'id'),
    (SELECT max(id) FROM rules.kinds),
    true
);

-- 2. Trigger-token column. Nullable BY DESIGN: only lazy_section rules carry a
--    token (NULL for every other kind), and the cross-column invariant
--    "lazy_section requires text + trigger_token" is owned by service-level
--    validation in the filter-rule CRUD path, not a DB CHECK — per the design
--    doc, and so that existing rows (all trigger_token IS NULL) stay valid.
ALTER TABLE rules.rules ADD COLUMN IF NOT EXISTS trigger_token TEXT;

COMMENT ON COLUMN rules.rules.trigger_token IS
    'Trigger token (e.g. !!CHROME!!) used only by lazy_section rules: its appearance in assistant text causes the stripped system-prompt section to be re-injected as a <system-reminder>. NULL for every other rule kind. The "lazy_section requires text + trigger_token" invariant is enforced by service-level validation, not a CHECK. See docs/plans/lazy-prompt-sections.md §Rule shape.';
