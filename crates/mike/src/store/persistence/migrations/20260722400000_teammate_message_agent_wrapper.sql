-- ════════════════════════════════════════════════════════════════════
-- prompt_kinds — teammate-message (3) also covers the subagent wrapper
--
-- The harness wraps injected peer/subagent text in two different tags,
-- both preceded by the same "Another Claude session sent a message:"
-- preamble:
--   <teammate-message teammate_id="...">   peer sessions — already matched
--   <agent-message from="...">             subagent reports — fell through
--                                          to the operator catch-all
--
-- Same provenance (another agent wrote this, not the operator), so this
-- extends kind 3 rather than adding a new kind. Matches the tag, not the
-- preamble sentence — the tag is the stable machine-emitted token; the
-- prose sentence can appear inside an operator's own message quoting it
-- (already observed as a false positive on session-recap in session
-- 09826143).
-- ════════════════════════════════════════════════════════════════════

-- ── Reclassify rows misfiled under the operator catch-all (4) ─────────────────
UPDATE prompt_kinds.classifications pkc
   SET kind_id = 3
  FROM memory.operator_turns ot
 WHERE pkc.request_id = ot.start_request_id
   AND pkc.kind_id = 4
   AND ot.user_prompt LIKE '%<agent-message from=%';
