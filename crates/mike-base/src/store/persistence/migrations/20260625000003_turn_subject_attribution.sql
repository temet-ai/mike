-- ════════════════════════════════════════════════════════════════════
-- Issue #25 (R3) — per-turn subject attribution
--
-- The eval harness needs to group captured turns by *subject* — which
-- command/skill and which agent was active — without regexing prompts
-- client-side. Two queryable columns on the operator-turn grain:
--
--   subject_command  the slash-command / skill that opened the turn,
--                    parsed from its user_prompt. Skill and command share
--                    one wire surface (a leading `/name`, e.g. `/implement`,
--                    `/review:fix`, `/thread:checkpoint`), so they fold into
--                    one column. NULL when the opening prompt is not a
--                    command invocation.
--   subject_agent    the agent persona active for the turn — the opening
--                    request's `subagent_persona` (extracted from the
--                    system prompt; see extract::request::extract_subagent_persona),
--                    defaulting to 'main'. Operator turns open only on
--                    primary/primary_first/steered origins (see
--                    store/persistence/mod.rs §3e), so this is 'main' in
--                    practice; the COALESCE keeps the mechanism correct and
--                    future-proof.
--
-- DRY: the slash-command extraction regex below is the single source of
-- truth. The capture-side write (store/persistence/mod.rs,
-- SUBJECT_ATTRIBUTION_UPDATE) embeds the SAME regex rather than
-- duplicating it in Rust string logic — mirroring the prompt_kinds CASE
-- pattern (migration 20260620000001).
-- ════════════════════════════════════════════════════════════════════

ALTER TABLE memory.operator_turns
    ADD COLUMN subject_command text,
    ADD COLUMN subject_agent   text;

-- Group-by read path: the harness groups turns by subject.
CREATE INDEX idx_operator_turns_subject_command
    ON memory.operator_turns USING btree (subject_command);

CREATE INDEX idx_operator_turns_subject_agent
    ON memory.operator_turns USING btree (subject_agent);

-- ── Backfill existing turns ───────────────────────────────────────────
--
-- subject_command: leading slash-command token in the turn's user_prompt.
-- The regex (single source of truth, mirrored capture-side) captures a
-- `/name` run — first char alphanumeric, then alphanumerics, `_`, `:`
-- (namespaced commands like `/thread:checkpoint`) or `-`. NULLIF maps the
-- no-match empty string to NULL.
UPDATE memory.operator_turns
   SET subject_command =
       NULLIF(substring(user_prompt FROM '^\s*(/[A-Za-z0-9][A-Za-z0-9_:-]*)'), '')
 WHERE user_prompt IS NOT NULL;

-- subject_agent: the opening request's already-extracted subagent_persona,
-- defaulting to 'main'. Set-based join on the turn's start request (the
-- request's id is the join key — never a content hash).
UPDATE memory.operator_turns ot
   SET subject_agent = COALESCE(r.subagent_persona, 'main')
  FROM requests.requests r
 WHERE r.id = ot.start_request_id;

-- Turns whose start request has been partition-evicted (no matching
-- requests row above): the active agent for an operator turn is the
-- primary, so default 'main'.
UPDATE memory.operator_turns
   SET subject_agent = 'main'
 WHERE subject_agent IS NULL;
