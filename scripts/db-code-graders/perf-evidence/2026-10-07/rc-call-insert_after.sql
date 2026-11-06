WITH
call_row AS (
    INSERT INTO remote_control.calls
        (channel_id, method_id, path, http_status,
         request_body, request_body_sha256, request_body_size,
         response_body, response_body_sha256, response_body_size,
         created_at, completed_at,
         payload_count, payload_kept_count, payload_type_counts,
         is_compaction, session_agent_id, preserved_event_ids)
    SELECT 866177::bigint, rm.id, '/worker/events'::text, 200::integer, NULL::bytea, NULL::bytea, NULL::integer,
           NULL::bytea, NULL::bytea, NULL::integer, '2026-10-07T09:30:00Z'::timestamptz::timestamptz, NULL::timestamptz,
           NULL::integer, NULL::integer, NULL::jsonb, NULL::boolean, NULL::text, NULL::text[]
      FROM requests.methods rm
     WHERE rm.code = 'POST'::text
    RETURNING id, created_at
),
transition AS (
    INSERT INTO remote_control.worker_status_transitions
        (channel_id, from_status, to_status, observed_at, call_id)
    SELECT 866177::bigint, 'idle'::text, 'idle'::text, NULL::timestamptz::timestamptz, call_row.id
      FROM call_row
     WHERE 'idle'::text IS DISTINCT FROM 'idle'::text
    RETURNING 1
),
kept_payloads AS (
    INSERT INTO remote_control.call_payloads
        (call_id, ordinal, payload_type, body, body_sha256, body_size, created_at)
    SELECT c.id, p.ordinal, p.payload_type, p.body, p.sha, p.size, c.created_at
      FROM call_row c
      CROSS JOIN UNNEST('{}'::int[]::int[], '{}'::text[]::text[], '{}'::bytea[]::bytea[],
                        '{}'::bytea[]::bytea[], '{}'::int[]::int[])
           AS p(ordinal, payload_type, body, sha, size)
    RETURNING 1
)
SELECT id AS "id!" FROM call_row;
