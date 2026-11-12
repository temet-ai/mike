SELECT max(finished_at) AS last_daily FROM wright.check_runs WHERE cadence = 'daily'
