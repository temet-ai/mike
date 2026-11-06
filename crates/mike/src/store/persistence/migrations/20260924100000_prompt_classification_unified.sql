-- ════════════════════════════════════════════════════════════════════
-- JC01: one prompt classifier — widen prompt_kinds, mark machine segments
--
-- Version 20260924100000 (assigned by the JC01 landing; above every migration on origin).
--
-- Why. Four copies of "is this the operator speaking" disagree: the LIKE set
-- that opens turns (turn_lifecycle.rs), a second copy on the abort path
-- (request_lifecycle_close.rs), the CASE that fills
-- prompt_kinds.classifications (operator_turns/judgment.rs), and
-- steerer/machine_text.rs. None knows the loop-wakeup templates, the
-- repetition rule or the agent-lineage rule that the fleet-mirror J2
-- resolver verified (night-watch-notes threads/fleet-mirror/J2-notes.md,
-- rework 2). In the J2 window 1,305 of 4,485 closed turns were opened by
-- machine text. JC01 ports J2 into one Rust classifier
-- (crates/mike/src/dossier/genuine) and stores its result here.
--
-- What.
--   1. Eight new prompt kinds. Existing ids 1-9 keep their codes; code
--      that reads kinds by id (the old CASE) keeps working.
--   2. prompt_kinds.classifications gains the capture rule that fired
--      ('a' template, 'c' harness author or agent lineage, 'envelope'
--      when the stripped prompt was empty and the raw last user message
--      carried a machine envelope), the repetition key used by rule (b'),
--      and the classifier version. Rule (b') itself is never stored: it
--      depends on other messages and on the moment it is evaluated, so the
--      builder (JC02) computes it as of any time from these rows.
--   3. memory.operator_turns.opened_by: 'operator' for a turn opened by a
--      genuine operator prompt, 'machine' for a segment opened by an
--      in-lane machine event (loop wakeup, peer message, hook feedback,
--      background event, compaction...), 'agent' for a historical row
--      opened before JC01 by a request carrying an agent id (written only
--      by mike origin-backfill and JC02's fill; no live path opens one).
--      NULL on rows written before JC01 until those backfills. Readers that
--      mean "the operator's turn" filter
--      (opened_by IS NULL OR opened_by = 'operator').
--
-- repeat_key is a normalised grouping key (lowercased, UUIDs and digits
-- removed, whitespace collapsed, first 120 characters, only for prompts of
-- 40 characters or more). It is not a copy of content for display: the full
-- prompt stays in requests.requests.human_prompt
-- (.claude/rules/inspection-fidelity.md).
--
-- No new index: the builder reads a session's rows through
-- idx_pk_classifications_session_id, and the judge claim keeps
-- idx_operator_turns_unjudged.
--
-- Expand-only. An older binary writes kind ids 1-9 with NULL rule and
-- opens turns with NULL opened_by, which new readers treat as an
-- operator turn. Idempotent: IF NOT EXISTS and ON CONFLICT DO NOTHING.
-- ════════════════════════════════════════════════════════════════════

INSERT INTO prompt_kinds.kinds (id, code) OVERRIDING SYSTEM VALUE VALUES
    (10, 'loop-wakeup'),
    (11, 'channel-wake'),
    (12, 'task-notification'),
    (13, 'tool-loaded'),
    (14, 'mailbox-delivery'),
    (15, 'steering-payload'),
    (16, 'harness-authored'),
    (17, 'agent-lineage')
ON CONFLICT DO NOTHING;

SELECT setval(
    pg_get_serial_sequence('prompt_kinds.kinds', 'id'),
    GREATEST((SELECT max(id) FROM prompt_kinds.kinds), 1)
);

SET LOCAL lock_timeout = '5s';

ALTER TABLE prompt_kinds.classifications
    ADD COLUMN IF NOT EXISTS rule text,
    ADD COLUMN IF NOT EXISTS repeat_key text,
    ADD COLUMN IF NOT EXISTS classifier_version text;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'classifications_rule_check') THEN
        ALTER TABLE prompt_kinds.classifications ADD CONSTRAINT classifications_rule_check CHECK (rule IN ('a', 'c', 'envelope')) NOT VALID;
    END IF;
END $$;
ALTER TABLE prompt_kinds.classifications VALIDATE CONSTRAINT classifications_rule_check;

COMMENT ON COLUMN prompt_kinds.classifications.rule IS
    'Capture rule that set kind_id: a = J2 template, c = harness author or agent-id lineage, envelope = stripped prompt empty and the raw last user message carried a machine envelope. NULL for genuine prompts and for rows written before JC01. Rule (b'') repetition is never stored here; it is evaluated as of a moment by the dossier builder.';
COMMENT ON COLUMN prompt_kinds.classifications.repeat_key IS
    'J2 repetition key: lower(prompt) with UUIDs and digits removed, whitespace collapsed, spaces trimmed, first 120 characters; NULL when the prompt is shorter than 40 characters. A grouping key, not display content; the full prompt is requests.requests.human_prompt.';
COMMENT ON COLUMN prompt_kinds.classifications.classifier_version IS
    'crate::dossier::genuine::CLASSIFIER_VERSION that wrote kind_id and rule; NULL for rows written by the pre-JC01 CASE.';

ALTER TABLE memory.operator_turns
    ADD COLUMN IF NOT EXISTS opened_by text;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'operator_turns_opened_by_check') THEN
        ALTER TABLE memory.operator_turns ADD CONSTRAINT operator_turns_opened_by_check CHECK (opened_by IN ('operator', 'machine', 'agent')) NOT VALID;
    END IF;
END $$;
ALTER TABLE memory.operator_turns VALIDATE CONSTRAINT operator_turns_opened_by_check;

COMMENT ON COLUMN memory.operator_turns.opened_by IS
    'operator = opened by a genuine operator prompt; machine = a segment opened by an in-lane machine event (loop wakeup, peer message, hook feedback, background event, compaction, tool-loaded, channel wake, envelope). agent = a historical row opened before JC01 by a request carrying an agent id (mike origin-backfill, JC02 fill). NULL for rows written before JC01 until those backfills. Since JC01, agent-lineage requests and harness probes never open a row.';
