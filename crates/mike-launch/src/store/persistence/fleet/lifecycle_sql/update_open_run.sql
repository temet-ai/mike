UPDATE launch.runs
   SET project_path = COALESCE($2, project_path),
       backend = $3,
       workspace_ref = COALESCE($4, workspace_ref),
       tab_ref = COALESCE($5, tab_ref)
 WHERE run_id = $1
   AND closed_at IS NULL
RETURNING run_id
