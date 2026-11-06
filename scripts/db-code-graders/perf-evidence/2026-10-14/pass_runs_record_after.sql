INSERT INTO steering.threshold_pass_runs
        (rollup_id, outcome, threshold_mode, rows_written, applied, n_by_source, detail)
 VALUES (233, 'wrote', 'apply', 276, 2, '{"relapse": {"session_reply": 2}}'::jsonb, '{}'::jsonb)
 ON CONFLICT (rollup_id) DO UPDATE
    SET last_ran_at = now(),
        attempts = steering.threshold_pass_runs.attempts + 1,
        outcome = EXCLUDED.outcome,
        threshold_mode = EXCLUDED.threshold_mode,
        rows_written = EXCLUDED.rows_written,
        applied = EXCLUDED.applied,
        n_by_source = EXCLUDED.n_by_source,
        detail = EXCLUDED.detail;
