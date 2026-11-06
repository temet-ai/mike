SELECT role,
       machine_uuid,
       heartbeat_at,
       EXTRACT(EPOCH FROM now() - heartbeat_at)::float8 AS age_secs
  FROM llm.worker_leases
 ORDER BY role
