INSERT INTO memory.lesson_proposal_jobs
                (verdict_id, candidate, proposed, "similar", model, cost_usd, generation_id, status)
             VALUES (1, '{}'::jsonb, '{}'::jsonb, '[]'::jsonb, 'perf-rollback', NULL::float8, NULL::text, 'awaiting_embedding')
             ON CONFLICT (verdict_id) DO NOTHING