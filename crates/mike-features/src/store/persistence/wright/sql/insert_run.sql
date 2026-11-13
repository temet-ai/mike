INSERT INTO wright.check_runs (check_name, source, cadence, started_at, outcome, findings, error, machine_uuid)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
RETURNING id
