SELECT run_id, project_path, backend, workspace_ref, tab_ref, created_at, closed_at
  FROM launch.runs
 WHERE run_id = $1
