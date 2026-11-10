UPDATE memory.operator_turns t
   SET embedded_at = NULL
 WHERE t.embedded_at IS NOT NULL
   AND t.closed_at IS NOT NULL
   AND (t.opened_by IS NULL OR t.opened_by = 'operator')
   AND (btrim(COALESCE(t.user_prompt, '')) <> ''
        OR btrim(COALESCE(t.final_assistant_text, '')) <> '')
   AND NOT EXISTS (SELECT 1 FROM memory.operator_turn_chunks c WHERE c.turn_id = t.id)
