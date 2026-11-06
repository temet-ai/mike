WITH ch AS (
    INSERT INTO remote_control.channels AS c
        (cse_id, client_session_id, worker_epoch, first_seen_at, last_seen_at,
         last_heartbeat_at, last_idle_seconds, last_heartbeat_interval_seconds,
         worker_status, worker_status_at,
         last_delivery_at, stream_opened_at)
    VALUES ('cse_c4ca4238a0b923820dcc509a6f75849b', 'cse_c4ca4238a0b923820dcc509a6f75849b', 7, now(), now(), now(), 137, 20, NULL::text, NULL::timestamptz, NULL::timestamptz, NULL::timestamptz)
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
        stream_opened_at  = GREATEST(c.stream_opened_at, EXCLUDED.stream_opened_at)
    RETURNING c.id
)
INSERT INTO remote_control.calls
    (channel_id, method_id, path, http_status,
     request_body, request_body_sha256, request_body_size,
     response_body, response_body_sha256, response_body_size,
     created_at, completed_at)
SELECT ch.id, rm.id, '/worker/heartbeat', 200, '\x7b2273657373696f6e5f6964223a226373655f6334636134323338613062393233383230646363353039613666373538343962222c22776f726b65725f65706f6368223a372c22737570706f7274735f6865617274626561745f70726f6265223a747275652c2263757272656e745f696e74657276616c5f7365636f6e6473223a32302c2269646c655f7365636f6e6473223a3133377d'::bytea, sha256('probe'::bytea), 151, NULL::bytea, NULL::bytea, NULL::integer, now(), now()
  FROM ch, requests.methods rm
 WHERE rm.code = 'POST'
RETURNING id
