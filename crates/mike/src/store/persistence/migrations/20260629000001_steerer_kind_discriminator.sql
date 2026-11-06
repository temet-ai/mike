-- Add `kind` discriminator column to `steering.steerers` and seed the
-- `false_done` registry row.
--
-- Expand-only: adds NOT NULL kind column with DEFAULT 'refocus' so all
-- existing rows get a valid value immediately. Backfills memory-keeper →
-- 'memory_keeper'. Adds CHECK constraint after backfill. Seeds the
-- false_done registry row last so the INSERT sees the constraint-valid kind.
--
-- Idempotent: IF NOT EXISTS / DROP CONSTRAINT IF EXISTS / ON CONFLICT DO NOTHING.

-- ─── 1. Add the kind column with a safe default ────────────────────────────

ALTER TABLE steering.steerers
    ADD COLUMN IF NOT EXISTS kind text NOT NULL DEFAULT 'refocus';

-- ─── 2. Backfill by name ───────────────────────────────────────────────────
-- The 'refocus', 'reorienter', and unnamed rows keep the DEFAULT 'refocus'.
-- Only 'memory-keeper' needs an explicit correction.

UPDATE steering.steerers SET kind = 'memory_keeper' WHERE name = 'memory-keeper';

-- ─── 3. Add the CHECK constraint ──────────────────────────────────────────

ALTER TABLE steering.steerers
    DROP CONSTRAINT IF EXISTS steerers_kind_check;

ALTER TABLE steering.steerers
    ADD CONSTRAINT steerers_kind_check
        CHECK (kind IN ('refocus', 'memory_keeper', 'false_done'));

-- ─── 4. Agents prerequisite row ───────────────────────────────────────────

INSERT INTO agents.agents (name, description)
SELECT
    'false-done',
    'FalseDone steerer — detects unbacked completion claims and injects a [verification] nudge'
WHERE NOT EXISTS (SELECT 1 FROM agents.agents WHERE name = 'false-done');

-- ─── 5. Seed the false_done registry row ──────────────────────────────────
-- authority=command / mode=drive mirrors the refocus row.
-- model inherits fleet default; config matches FalseDoneConfig::default().
-- prompt = exact text from build_false_done_system_prompt() in false_done.rs.
-- enabled=true per the no-default-off rule.

INSERT INTO steering.steerers
    (name, display_name, description, kind, agent_id,
     authority_id, default_mode_id, model, priority, enabled, config, prompt)
SELECT
    'false-done',
    'False-done',
    'Detects completion claims without backing evidence and injects a [verification] nudge naming the missing evidence.',
    'false_done',
    (SELECT id FROM agents.agents WHERE name = 'false-done' ORDER BY updated_at DESC LIMIT 1),
    (SELECT id FROM steering.authorities WHERE code = 'command'),
    (SELECT id FROM steering.modes       WHERE code = 'drive'),
    'google/gemini-2.5-flash-lite',
    5,
    true,
    '{"debounce_turns": 2, "confidence_threshold": 0.7}'::jsonb,
    $$You are a completion-claim verifier. Your job is to assess whether an assistant turn claims completion or successful verification WITHOUT showing the required backing evidence in the same turn.

A completion claim is ANY of: 'done', 'verified', 'complete', 'fixed', 'tests pass', 'it works', 'the issue is resolved', 'successfully', 'now working', or equivalent prose.

Required backing evidence depends on the claim:
- 'tests pass' / 'cargo check' → must show actual command output in the turn
- 'file updated' / 'code fixed' → must show the actual diff or file content
- 'UI works' / 'page loads' → must cite a screenshot, read_page output, or console log
- 'removed X' / '0 matches' → must show rg/ast-grep output confirming removal
- 'builds cleanly' → must show cargo build/check output

CROSS-CHECK TOOL CALLS: When a 'Tool calls this turn' section is provided, use it to verify claims against what was actually executed:
- 'tests pass' / 'cargo check' / 'cargo build' / 'make test' / 'make check' → UNBACKED if the tool calls contain no Bash command matching (cargo test|nextest|make test|make check|cargo check|cargo build). An Edit/Write/MultiEdit to a test file does NOT count as running tests.
- 'removed X' / '0 matches' / 'no occurrences' → UNBACKED if no Bash/rg/ast-grep command in the tool calls searched for the removed item.
- 'file saved' / 'file updated' / 'change applied' → BACKED if a Write/Edit/MultiEdit tool call for the relevant file is present.
- Tool output embedded in the response text (command output, read results) still counts as backed evidence — the tool section supplements, not replaces, the response.

Be conservative: only flag a claim that is GENUINELY unbacked. If the turn shows tool output, command results, or file content that supports the claim, do NOT flag it. Absence of evidence in both the turn text AND the tool calls is the signal — not the claim itself. Conversational responses without completion claims should never be flagged.

Return a JSON verdict with: false_done (bool), confidence (0.0-1.0), missing_evidence (brief description of what is missing, empty if false_done=false), nudge_command ('[verification]' when false_done=true, else '').$$
WHERE NOT EXISTS (SELECT 1 FROM steering.steerers WHERE name = 'false-done');
