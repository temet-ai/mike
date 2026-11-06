-- ════════════════════════════════════════════════════════════════════
-- prompt_kinds — four harness-generated families, split out of `operator`
--
-- Claude-Code-injected text (session recaps, background notifications,
-- compaction summaries, analysis probes) was falling into the `operator`
-- catch-all, so the SPA rendered it as an operator-typed turn. None of
-- these strings are ever operator-typed — they are Claude Code / external
-- tooling text.
--
-- New kinds (ids continue from the seeded 1-4 in 20260620000001_prompt_kind.sql):
--   5 session-recap    "The user stepped away and is coming back"
--   6 background-event "[SYSTEM NOTIFICATION - NOT USER INPUT]"
--   7 compaction       "This session is being continued from a previous conversation"
--                       OR "Summarize this portion of a Claude Code session transcript"
--   8 analysis-probe   "Analyze this Claude Code session and extract structured facets"
--                       OR "has the following stopping condition been satisfied"
--
-- CASE order (first-match-wins): 1, 2, 3 unchanged, then 5, 6, 7, 8, then
-- ELSE 4. No pattern overlaps. The capture-side Rust INSERT
-- (operator_turns.rs::write_prompt_kind_classification) embeds the
-- identical CASE rather than duplicating the strings differently.
-- ════════════════════════════════════════════════════════════════════

INSERT INTO prompt_kinds.kinds (id, code) OVERRIDING SYSTEM VALUE VALUES
    (5, 'session-recap'),
    (6, 'background-event'),
    (7, 'compaction'),
    (8, 'analysis-probe');

-- ── Reclassify rows misfiled under the operator catch-all (4) ─────────────────
--
-- Scoped to rows already classified as 4 whose source `user_prompt` matches
-- one of the new families; rows classified 1/2/3 are never touched.
UPDATE prompt_kinds.classifications pkc
   SET kind_id = CASE
                   WHEN ot.user_prompt LIKE '%The user stepped away and is coming back%' THEN 5
                   WHEN ot.user_prompt LIKE '%[SYSTEM NOTIFICATION - NOT USER INPUT]%' THEN 6
                   WHEN ot.user_prompt LIKE '%This session is being continued from a previous conversation%' THEN 7
                   WHEN ot.user_prompt LIKE '%Summarize this portion of a Claude Code session transcript%' THEN 7
                   WHEN ot.user_prompt LIKE '%Analyze this Claude Code session and extract structured facets%' THEN 8
                   WHEN ot.user_prompt LIKE '%has the following stopping condition been satisfied%' THEN 8
                 END
  FROM memory.operator_turns ot
 WHERE pkc.request_id = ot.start_request_id
   AND pkc.kind_id = 4
   AND (
        ot.user_prompt LIKE '%The user stepped away and is coming back%'
     OR ot.user_prompt LIKE '%[SYSTEM NOTIFICATION - NOT USER INPUT]%'
     OR ot.user_prompt LIKE '%This session is being continued from a previous conversation%'
     OR ot.user_prompt LIKE '%Summarize this portion of a Claude Code session transcript%'
     OR ot.user_prompt LIKE '%Analyze this Claude Code session and extract structured facets%'
     OR ot.user_prompt LIKE '%has the following stopping condition been satisfied%'
   );
