INSERT INTO workspace.code_scans
       (change_event_id, checkout, path, language, rules_version, rules_run, bytes,
        duration_us, skipped, error, findings_before, findings_after, findings_new, findings_fixed)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, 0)
ON CONFLICT (change_event_id) DO NOTHING
RETURNING id
