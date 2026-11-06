INSERT INTO memory.lesson_proposal_evidence
    (lesson_id, verdict_id, operator_turn_id, similar_corrections, model, cost_usd, generation_id, reason)
    VALUES (NULL::bigint, 1, 17784, '[]'::jsonb, 'perf-rollback', NULL::float8, NULL::text, 'performance check rolled back')
    ON CONFLICT (verdict_id) DO UPDATE
    SET lesson_id = EXCLUDED.lesson_id, similar_corrections = EXCLUDED.similar_corrections,
        model = EXCLUDED.model, cost_usd = EXCLUDED.cost_usd,
        generation_id = EXCLUDED.generation_id, reason = EXCLUDED.reason,
        created_at = EXCLUDED.created_at
    WHERE EXISTS (SELECT 1 FROM memory.lesson_proposal_recovery recovery
                  WHERE recovery.verdict_id = EXCLUDED.verdict_id
                    AND recovery.recovery_state = 'needs_reproposal')
    RETURNING id