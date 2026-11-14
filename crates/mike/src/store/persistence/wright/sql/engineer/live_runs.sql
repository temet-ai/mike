SELECT id, state, host, machine_uuid, pid, deadline_at
  FROM wright.investigations
 WHERE state IN ('starting', 'running')
