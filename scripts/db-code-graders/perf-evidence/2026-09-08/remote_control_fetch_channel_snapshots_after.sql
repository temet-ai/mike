SELECT
    c.cse_id,
    c.local_session_uuid,
    c.last_heartbeat_at,
    c.last_heartbeat_interval_seconds,
    c.last_idle_seconds,
    c.worker_status,
    c.worker_status_at,
    (c.stream_opened_at IS NOT NULL
       AND (c.stream_closed_at IS NULL
            OR c.stream_closed_at < c.stream_opened_at)) AS stream_open,
    c.stream_opened_at,
    c.last_delivery_at,
    c.post_turn_summary,
    c.post_turn_summary_at,
    c.client_version
  FROM remote_control.channels c
 WHERE c.local_session_uuid = ANY(ARRAY[
    '00000000-0000-4000-8000-0000000003e8',
    '00000000-0000-4000-8000-0000000003e9',
    '00000000-0000-4000-8000-0000000003ea',
    '00000000-0000-4000-8000-0000000003eb',
    '00000000-0000-4000-8000-0000000003ec',
    '00000000-0000-4000-8000-0000000003ed',
    '00000000-0000-4000-8000-0000000003ee',
    '00000000-0000-4000-8000-0000000003ef',
    '00000000-0000-4000-8000-0000000003f0',
    '00000000-0000-4000-8000-0000000003f1'
 ]::uuid[])
