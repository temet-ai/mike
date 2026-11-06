UPDATE launch.tree_checks
   SET finished_at = now(),
       exit_code = $2,
       output = $3,
       state_after = $4
 WHERE id = $1
