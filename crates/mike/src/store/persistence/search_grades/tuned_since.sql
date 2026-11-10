SELECT EXISTS (SELECT 1 FROM memory.search_tuning_runs r WHERE r.ran_at >= $1) AS "tuned!"
