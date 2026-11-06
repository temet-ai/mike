INSERT INTO launch.runs AS r (run_id, project_path, backend, workspace_ref, tab_ref)
VALUES ($1, $2, $3, $4, $5)
ON CONFLICT (run_id) DO UPDATE
   SET project_path  = COALESCE(EXCLUDED.project_path, r.project_path),
       backend       = EXCLUDED.backend,
       workspace_ref = COALESCE(EXCLUDED.workspace_ref, r.workspace_ref),
       tab_ref       = COALESCE(EXCLUDED.tab_ref, r.tab_ref),
       closed_at     = NULL
