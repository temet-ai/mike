WITH
old AS (
    SELECT worker_status FROM remote_control.channels WHERE cse_id = 'cse_1000'
    UNION ALL
    SELECT NULL::text
     WHERE NOT EXISTS (
        SELECT 1 FROM remote_control.channels WHERE cse_id = 'cse_1000'
     )
),
ch AS (
    INSERT INTO remote_control.channels AS c
        (cse_id, client_session_id, worker_epoch, first_seen_at, last_seen_at,
         last_heartbeat_at, last_idle_seconds, last_heartbeat_interval_seconds,
         worker_status, worker_status_at,
         last_delivery_at, stream_opened_at,
         local_session_uuid, local_cwd, client_version,
         post_turn_summary, post_turn_summary_at, session_id)
    VALUES ('cse_1000', NULL, 1, now(), now(), now(), 5, 20,
            'requires_action', now(), now(), NULL::timestamptz,
            '00000000-0000-4000-8000-0000000003e8'::uuid, '/repo/1000', '2.1.263',
            NULL::jsonb, NULL::timestamptz,
            (SELECT s.id FROM sessions.sessions s
              WHERE s.session_uuid = '00000000-0000-4000-8000-0000000003e8'::uuid))
    ON CONFLICT (cse_id) DO UPDATE SET
        client_session_id = COALESCE(EXCLUDED.client_session_id, c.client_session_id),
        worker_epoch      = COALESCE(EXCLUDED.worker_epoch, c.worker_epoch),
        last_seen_at      = GREATEST(c.last_seen_at, EXCLUDED.last_seen_at),
        last_heartbeat_at = GREATEST(c.last_heartbeat_at, EXCLUDED.last_heartbeat_at),
        last_idle_seconds = CASE
            WHEN EXCLUDED.last_heartbeat_at IS NOT NULL
             AND (c.last_heartbeat_at IS NULL
                  OR EXCLUDED.last_heartbeat_at >= c.last_heartbeat_at)
            THEN EXCLUDED.last_idle_seconds ELSE c.last_idle_seconds END,
        last_heartbeat_interval_seconds = CASE
            WHEN EXCLUDED.last_heartbeat_at IS NOT NULL
             AND (c.last_heartbeat_at IS NULL
                  OR EXCLUDED.last_heartbeat_at >= c.last_heartbeat_at)
            THEN EXCLUDED.last_heartbeat_interval_seconds
            ELSE c.last_heartbeat_interval_seconds END,
        worker_status     = CASE
            WHEN EXCLUDED.worker_status_at IS NOT NULL
             AND (c.worker_status_at IS NULL
                  OR EXCLUDED.worker_status_at >= c.worker_status_at)
            THEN EXCLUDED.worker_status ELSE c.worker_status END,
        worker_status_at  = GREATEST(c.worker_status_at, EXCLUDED.worker_status_at),
        last_delivery_at  = GREATEST(c.last_delivery_at, EXCLUDED.last_delivery_at),
        stream_opened_at  = GREATEST(c.stream_opened_at, EXCLUDED.stream_opened_at),
        local_session_uuid = COALESCE(c.local_session_uuid, EXCLUDED.local_session_uuid),
        local_cwd = CASE WHEN c.local_session_uuid IS NULL
                         THEN EXCLUDED.local_cwd ELSE c.local_cwd END,
        client_version = CASE WHEN c.local_session_uuid IS NULL
                              THEN EXCLUDED.client_version ELSE c.client_version END,
        session_id = COALESCE(c.session_id,
            (SELECT s.id FROM sessions.sessions s
              WHERE s.session_uuid = COALESCE(c.local_session_uuid, EXCLUDED.local_session_uuid))),
        post_turn_summary = CASE
            WHEN EXCLUDED.post_turn_summary_at IS NOT NULL
             AND (c.post_turn_summary_at IS NULL
                  OR EXCLUDED.post_turn_summary_at >= c.post_turn_summary_at)
            THEN EXCLUDED.post_turn_summary ELSE c.post_turn_summary END,
        post_turn_summary_at = GREATEST(c.post_turn_summary_at, EXCLUDED.post_turn_summary_at)
    RETURNING c.id, c.worker_status, c.worker_status_at
),
call_row AS (
    INSERT INTO remote_control.calls
        (channel_id, method_id, path, http_status,
         request_body, request_body_sha256, request_body_size,
         response_body, response_body_sha256, response_body_size,
         created_at, completed_at,
         payload_count, payload_kept_count, payload_type_counts,
         is_compaction, session_agent_id, preserved_event_ids)
    SELECT ch.id, rm.id, '/worker', 200,
           '\x7b7d'::bytea, sha256('\x7b7d'::bytea), 2,
           '\x7b7d'::bytea, sha256('\x7b7d'::bytea), 2,
           now(), now(),
           1, 1, '{"attachment":1}'::jsonb,
           false, NULL, NULL::text[]
      FROM ch, requests.methods rm
     WHERE rm.code = 'PUT'
    RETURNING id, created_at
),
transition AS (
    INSERT INTO remote_control.worker_status_transitions
        (channel_id, from_status, to_status, observed_at, call_id)
    SELECT ch.id, old.worker_status, ch.worker_status, ch.worker_status_at, call_row.id
      FROM ch, old, call_row
     WHERE old.worker_status IS DISTINCT FROM ch.worker_status
    RETURNING 1
),
kept_payloads AS (
    INSERT INTO remote_control.call_payloads
        (call_id, ordinal, payload_type, body, body_sha256, body_size, created_at)
    SELECT c.id, p.ordinal, p.payload_type, p.body, p.sha, p.size, c.created_at
      FROM call_row c
      CROSS JOIN UNNEST(ARRAY[0]::int[], ARRAY['attachment']::text[],
                        ARRAY['\x7b7d'::bytea]::bytea[],
                        ARRAY[sha256('\x7b7d'::bytea)]::bytea[], ARRAY[2]::int[])
           AS p(ordinal, payload_type, body, sha, size)
    RETURNING 1
)
SELECT id FROM call_row
