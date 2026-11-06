SELECT id, version, ask_entry_count, model, output_tokens, created_at
  FROM memory.acceptance_criteria_maps
 WHERE session_id = 1139628
 ORDER BY version DESC
 LIMIT 1
