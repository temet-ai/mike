SELECT rollup_id, first_ran_at, last_ran_at, attempts, outcome, threshold_mode,
        rows_written, applied, n_by_source, detail
   FROM steering.threshold_pass_runs
  ORDER BY rollup_id DESC
  LIMIT 30;
