-- end_workspace_registration closed a session's open operator turn without materializing its
-- aggregates, so closed turns kept NULL tool counts, tokens and tool/file/skill/agent lists
-- (241 on 2026-11-02). The close path now materializes in the same transaction; this fills in
-- the turns it missed. Set-based and idempotent: a turn that already has aggregates is not
-- selected.
SELECT memory.refresh_inherited_turns(array_agg(id))
  FROM memory.operator_turns
 WHERE closed_at IS NOT NULL
   AND tool_call_count IS NULL;
