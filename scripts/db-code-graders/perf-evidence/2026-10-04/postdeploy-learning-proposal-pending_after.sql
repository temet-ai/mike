SELECT jsonb_build_object('candidate', candidate, 'proposed', proposed,
                'similar', "similar", 'model', model, 'cost_usd', cost_usd,
                'generation_id', generation_id) AS "saved!: sqlx::types::Json<SavedProposal>"
             FROM memory.lesson_proposal_jobs
             WHERE status = 'awaiting_embedding' AND next_attempt_at <= NOW()
             ORDER BY updated_at, verdict_id LIMIT 40