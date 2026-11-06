SELECT
    t.channel_id,
    t.from_status,
    t.to_status,
    t.observed_at,
    t.call_id
  FROM remote_control.worker_status_transitions t
  JOIN remote_control.channels c ON c.id = t.channel_id
 WHERE c.local_session_uuid = '00000000-0000-4000-8000-0000000003e8'
 ORDER BY t.observed_at DESC
 LIMIT 50
