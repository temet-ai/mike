SELECT id, backend, passed, version, protocol, checks, started_at, finished_at, source
  FROM launch.backend_validations
 WHERE backend = $1
 ORDER BY finished_at DESC, id DESC
 LIMIT 1
