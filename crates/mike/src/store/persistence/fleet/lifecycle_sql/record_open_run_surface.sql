UPDATE launch.runs
   SET workspace_ref = COALESCE($2, workspace_ref),
       tab_ref = COALESCE($3, tab_ref)
 WHERE run_id = $1
   AND closed_at IS NULL
RETURNING run_id
