SELECT id FROM memory.operator_turns WHERE closed_at IS NOT NULL AND embedded_at IS NULL AND (opened_by IS NULL OR opened_by = 'operator') ORDER BY closed_at ASC LIMIT 32
