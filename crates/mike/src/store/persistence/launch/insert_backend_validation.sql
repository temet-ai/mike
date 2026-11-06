INSERT INTO launch.backend_validations (
    backend, passed, version, protocol, checks, started_at, finished_at, source
)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
RETURNING id
