WITH cur AS (
    SELECT id, worker_status, worker_status_at
      FROM remote_control.channels
     WHERE cse_id = 'cse_018FPyV6PkmjVHsTT9PbVoqf'::text
),
upd AS (
    UPDATE remote_control.channels c SET
        client_session_id = COALESCE(NULL::text, c.client_session_id),
        worker_epoch      = COALESCE(NULL::bigint, c.worker_epoch),
        last_seen_at      = GREATEST(c.last_seen_at, '2026-10-07T09:30:00Z'::timestamptz::timestamptz),
        last_heartbeat_at = GREATEST(c.last_heartbeat_at, NULL::timestamptz),
        last_idle_seconds = CASE
            WHEN NULL::timestamptz IS NOT NULL
             AND (c.last_heartbeat_at IS NULL OR NULL::timestamptz >= c.last_heartbeat_at)
            THEN NULL::integer ELSE c.last_idle_seconds END,
        last_heartbeat_interval_seconds = CASE
            WHEN NULL::timestamptz IS NOT NULL
             AND (c.last_heartbeat_at IS NULL OR NULL::timestamptz >= c.last_heartbeat_at)
            THEN NULL::integer ELSE c.last_heartbeat_interval_seconds END,
        worker_status     = CASE
            WHEN NULL::timestamptz IS NOT NULL
             AND (c.worker_status_at IS NULL OR NULL::timestamptz >= c.worker_status_at)
            THEN NULL::text ELSE c.worker_status END,
        worker_status_at  = GREATEST(c.worker_status_at, NULL::timestamptz),
        last_delivery_at  = GREATEST(c.last_delivery_at, NULL::timestamptz),
        stream_opened_at  = GREATEST(c.stream_opened_at, NULL::timestamptz),
        local_session_uuid = COALESCE(c.local_session_uuid, NULL::uuid::uuid),
        local_cwd = CASE WHEN c.local_session_uuid IS NULL THEN NULL::text ELSE c.local_cwd END,
        client_version = CASE WHEN c.local_session_uuid IS NULL THEN NULL::text ELSE c.client_version END,
        session_id = COALESCE(c.session_id,
            (SELECT s.id FROM sessions.sessions s
              WHERE s.session_uuid = COALESCE(c.local_session_uuid, NULL::uuid::uuid))),
        post_turn_summary = CASE
            WHEN NULL::timestamptz IS NOT NULL
             AND (c.post_turn_summary_at IS NULL OR NULL::timestamptz >= c.post_turn_summary_at)
            THEN NULL::jsonb ELSE c.post_turn_summary END,
        post_turn_summary_at = GREATEST(c.post_turn_summary_at, NULL::timestamptz)
     WHERE c.cse_id = 'cse_018FPyV6PkmjVHsTT9PbVoqf'::text
       AND (
            (NULL::text IS NOT NULL AND NULL::text IS DISTINCT FROM c.client_session_id)
         OR (NULL::bigint IS NOT NULL AND NULL::bigint IS DISTINCT FROM c.worker_epoch)
         OR '2026-10-07T09:30:00Z'::timestamptz::timestamptz > c.last_seen_at + make_interval(secs => 10::integer)
         OR (NULL::timestamptz IS NOT NULL
             AND (c.last_heartbeat_at IS NULL OR NULL::timestamptz > c.last_heartbeat_at))
         OR (NULL::timestamptz IS NOT NULL
             AND (c.worker_status_at IS NULL OR NULL::timestamptz >= c.worker_status_at)
             AND NULL::text IS DISTINCT FROM c.worker_status)
         OR (NULL::timestamptz IS NOT NULL
             AND (c.last_delivery_at IS NULL OR NULL::timestamptz > c.last_delivery_at))
         OR (NULL::timestamptz IS NOT NULL
             AND (c.stream_opened_at IS NULL OR NULL::timestamptz > c.stream_opened_at))
         OR (NULL::uuid::uuid IS NOT NULL AND c.local_session_uuid IS NULL)
         OR (c.session_id IS NULL
             AND EXISTS (SELECT 1 FROM sessions.sessions s
                          WHERE s.session_uuid = COALESCE(c.local_session_uuid, NULL::uuid::uuid)))
         OR (NULL::timestamptz IS NOT NULL
             AND (c.post_turn_summary_at IS NULL OR NULL::timestamptz >= c.post_turn_summary_at)
             AND NULL::jsonb IS DISTINCT FROM c.post_turn_summary)
       )
    RETURNING c.id, c.worker_status, c.worker_status_at
),
ins AS (
    INSERT INTO remote_control.channels
        (cse_id, client_session_id, worker_epoch, first_seen_at, last_seen_at,
         last_heartbeat_at, last_idle_seconds, last_heartbeat_interval_seconds,
         worker_status, worker_status_at, last_delivery_at, stream_opened_at,
         local_session_uuid, local_cwd, client_version,
         post_turn_summary, post_turn_summary_at, session_id)
    SELECT 'cse_018FPyV6PkmjVHsTT9PbVoqf'::text, NULL::text, NULL::bigint, '2026-10-07T09:30:00Z'::timestamptz::timestamptz, '2026-10-07T09:30:00Z'::timestamptz::timestamptz,
           NULL::timestamptz, NULL::integer, NULL::integer,
           NULL::text, NULL::timestamptz, NULL::timestamptz, NULL::timestamptz,
           NULL::uuid::uuid, NULL::text, NULL::text, NULL::jsonb, NULL::timestamptz,
           (SELECT s.id FROM sessions.sessions s WHERE s.session_uuid = NULL::uuid::uuid)
     WHERE NOT EXISTS (SELECT 1 FROM cur)
    ON CONFLICT (cse_id) DO NOTHING
    RETURNING id, worker_status, worker_status_at
)
SELECT COALESCE((SELECT id FROM upd), (SELECT id FROM ins), (SELECT id FROM cur)) AS id,
       (SELECT worker_status FROM cur) AS previous_status,
       COALESCE((SELECT worker_status FROM upd), (SELECT worker_status FROM ins),
                (SELECT worker_status FROM cur)) AS current_status,
       COALESCE((SELECT worker_status_at FROM upd), (SELECT worker_status_at FROM ins),
                (SELECT worker_status_at FROM cur)) AS current_status_at;
