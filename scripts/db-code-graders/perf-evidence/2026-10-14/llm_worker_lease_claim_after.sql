WITH claimed AS (
    INSERT INTO llm.worker_leases AS lease (role, machine_uuid, heartbeat_at)
    VALUES ('steerer', '3f6c7a52-91d4-4c1e-b7a0-5e2d8c4f1a90'::uuid, now())
    ON CONFLICT (role) DO UPDATE
       SET machine_uuid = EXCLUDED.machine_uuid,
           heartbeat_at = now()
     WHERE lease.machine_uuid = EXCLUDED.machine_uuid
        OR lease.heartbeat_at < now() - make_interval(secs => 180::float8)
 RETURNING 1
)
SELECT EXISTS (SELECT 1 FROM claimed) AS held
