SELECT (NOT EXISTS (
           SELECT 1
             FROM launch.launches
            WHERE run_id = $1
              AND ((opened AND (closed_at IS NULL OR close_process_exited IS DISTINCT FROM TRUE))
                   OR (NOT opened AND result <> 'failed'))
       )
       AND NOT EXISTS (
           SELECT 1
             FROM launch.tree_checks
            WHERE finished_at IS NULL
              AND (run_id = $1 OR starts_with(run_id, $1 || '#'))
       )) AS "settled!"
