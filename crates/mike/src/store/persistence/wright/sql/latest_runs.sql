SELECT DISTINCT ON (check_name)
       id, check_name, source, cadence, started_at, finished_at, outcome, findings, error
  FROM wright.check_runs
 ORDER BY check_name, finished_at DESC, id DESC
