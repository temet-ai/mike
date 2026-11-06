-- Issue #104: memory.operator_turns was counting harness/probe text and
-- empty prompts as genuine operator turns. Companion #105 (author_kind,
-- 20260811010000) is not the mechanism this issue needed -- the actual
-- gate is turn_origin/turn_type, already correctly restricted to
-- primary/primary_first user_prompt requests. What was missing is a
-- content-level filter within that gate: seven harness-generated prompt
-- families already have a proven classifier
-- (write_prompt_kind_classification, operator_turns.rs, mirrored from
-- 20260722020000_prompt_kind_harness_families.sql) that labels these rows
-- in prompt_kinds.classifications for display, but nothing stopped them
-- from opening a memory.operator_turns row in the first place.
--
-- This migration: (1) adds the 8th kind the classifier CASE never
-- covered -- empty user_prompt, which fell through to the 'operator'
-- catch-all; (2) reclassifies the now-covered empty-prompt rows;
-- (3) deletes every existing operator_turns row whose user_prompt matches
-- a harness pattern or is empty. The companion Rust change (this same
-- commit, store/persistence/mod.rs) adds the identical guard to both
-- turn-opening INSERTs so new rows of these kinds stop accumulating --
-- this migration only cleans up what already exists.
--
-- FK safety: memory.operator_turn_chunks and steering.evaluations
-- CASCADE (turn-derived artifacts correctly vanish with the turn);
-- memory.lessons.source_turn_id, requests.requests.operator_turn_id,
-- steering.interventions.operator_turn_id, memory.consolidation_audit
-- .source_turn_id, and memory.ask_ledger_entries.operator_turn_id all
-- SET NULL (the referencing row survives, detached from the deleted
-- turn). No table references memory.operator_turns with a blocking
-- RESTRICT/NO ACTION, so the DELETE below cannot fail on an FK.

INSERT INTO prompt_kinds.kinds (id, code) OVERRIDING SYSTEM VALUE VALUES
    (9, 'empty-prompt')
ON CONFLICT (id) DO NOTHING;

-- Reclassify rows misfiled under the operator catch-all (4) whose source
-- user_prompt is empty -- the one pattern write_prompt_kind_classification's
-- CASE never checked. Scoped to kind_id = 4 so already-correct
-- classifications (1/2/3/5/6/7/8) are never touched.
UPDATE prompt_kinds.classifications pkc
   SET kind_id = 9
  FROM memory.operator_turns ot
 WHERE pkc.request_id = ot.start_request_id
   AND pkc.kind_id = 4
   AND (ot.user_prompt IS NULL OR ot.user_prompt = '');

-- Delete the spurious turns. Matched directly against user_prompt (not via
-- prompt_kinds.classifications) so this DELETE is self-contained and does
-- not depend on every turn already having a classification row backfilled.
DELETE FROM memory.operator_turns ot
 WHERE ot.user_prompt IS NULL
    OR ot.user_prompt = ''
    OR ot.user_prompt LIKE '%Describe your most recent action in 3-5 words%'
    OR ot.user_prompt LIKE '%Stop hook feedback:%'
    OR ot.user_prompt LIKE '%<teammate-message%'
    OR ot.user_prompt LIKE '%<agent-message from=%'
    OR ot.user_prompt LIKE '%The user stepped away and is coming back%'
    OR ot.user_prompt LIKE '%[SYSTEM NOTIFICATION - NOT USER INPUT]%'
    OR ot.user_prompt LIKE '%This session is being continued from a previous conversation%'
    OR ot.user_prompt LIKE '%Summarize this portion of a Claude Code session transcript%'
    OR ot.user_prompt LIKE '%Analyze this Claude Code session and extract structured facets%'
    OR ot.user_prompt LIKE '%has the following stopping condition been satisfied%';
