-- The channel roll-up half of recording a batch of Remote Control calls on one
-- channel. The caller merges the batch's signals first (newest of each dated
-- signal, earliest created_at as $18, latest as $4), so a burst of calls costs
-- one roll-up statement, and record_calls.sql then inserts the calls.
--
-- The channel row is written only when the call changes something a reader
-- can see: a new status, a newer heartbeat, a newly opened stream, a newer
-- delivery, a first link to a session, a new post-turn summary, or a call
-- that is more than $17 seconds newer than last_seen_at. A call that changes
-- nothing (most /worker/events and /worker batches) takes no row lock at all,
-- because the UPDATE finds no row to write and the plain SELECT below does
-- not lock.
--
-- Bind parameters:
--   $1-$11   cse_id, client_session_id, worker_epoch, created_at,
--            heartbeat_at, idle_seconds, current_interval_seconds,
--            worker_status, worker_status_at, delivery_at, stream_opened_at
--   $12-$16  local_session_uuid, local_cwd, client_version,
--            post_turn_summary, post_turn_summary_at
--   $17      last_seen_at debounce, in seconds
--   $18      first_seen_at for a channel this statement creates
--
-- Always returns exactly one row. `id` is NULL only when a concurrent call
-- created the channel between this statement's snapshot and its insert; the
-- caller runs the statement again and the second pass finds the row.
WITH cur AS (
    SELECT id, worker_status, worker_status_at
      FROM remote_control.channels
     WHERE cse_id = $1::text
),
upd AS (
    UPDATE remote_control.channels c SET
        client_session_id = COALESCE($2::text, c.client_session_id),
        worker_epoch      = COALESCE($3::bigint, c.worker_epoch),
        last_seen_at      = GREATEST(c.last_seen_at, $4::timestamptz),
        last_heartbeat_at = GREATEST(c.last_heartbeat_at, $5::timestamptz),
        last_idle_seconds = CASE
            WHEN $5::timestamptz IS NOT NULL
             AND (c.last_heartbeat_at IS NULL OR $5::timestamptz >= c.last_heartbeat_at)
            THEN $6::integer ELSE c.last_idle_seconds END,
        last_heartbeat_interval_seconds = CASE
            WHEN $5::timestamptz IS NOT NULL
             AND (c.last_heartbeat_at IS NULL OR $5::timestamptz >= c.last_heartbeat_at)
            THEN $7::integer ELSE c.last_heartbeat_interval_seconds END,
        worker_status     = CASE
            WHEN $9::timestamptz IS NOT NULL
             AND (c.worker_status_at IS NULL OR $9::timestamptz >= c.worker_status_at)
            THEN $8::text ELSE c.worker_status END,
        worker_status_at  = GREATEST(c.worker_status_at, $9::timestamptz),
        last_delivery_at  = GREATEST(c.last_delivery_at, $10::timestamptz),
        stream_opened_at  = GREATEST(c.stream_opened_at, $11::timestamptz),
        local_session_uuid = COALESCE(c.local_session_uuid, $12::uuid),
        local_cwd = CASE WHEN c.local_session_uuid IS NULL THEN $13::text ELSE c.local_cwd END,
        client_version = CASE WHEN c.local_session_uuid IS NULL THEN $14::text ELSE c.client_version END,
        session_id = COALESCE(c.session_id,
            (SELECT s.id FROM sessions.sessions s
              WHERE s.session_uuid = COALESCE(c.local_session_uuid, $12::uuid))),
        post_turn_summary = CASE
            WHEN $16::timestamptz IS NOT NULL
             AND (c.post_turn_summary_at IS NULL OR $16::timestamptz >= c.post_turn_summary_at)
            THEN $15::jsonb ELSE c.post_turn_summary END,
        post_turn_summary_at = GREATEST(c.post_turn_summary_at, $16::timestamptz)
     WHERE c.cse_id = $1::text
       AND (
            ($2::text IS NOT NULL AND $2::text IS DISTINCT FROM c.client_session_id)
         OR ($3::bigint IS NOT NULL AND $3::bigint IS DISTINCT FROM c.worker_epoch)
         OR $4::timestamptz > c.last_seen_at + make_interval(secs => $17::integer)
         OR ($5::timestamptz IS NOT NULL
             AND (c.last_heartbeat_at IS NULL OR $5::timestamptz > c.last_heartbeat_at))
         OR ($9::timestamptz IS NOT NULL
             AND (c.worker_status_at IS NULL OR $9::timestamptz >= c.worker_status_at)
             AND $8::text IS DISTINCT FROM c.worker_status)
         OR ($10::timestamptz IS NOT NULL
             AND (c.last_delivery_at IS NULL OR $10::timestamptz > c.last_delivery_at))
         OR ($11::timestamptz IS NOT NULL
             AND (c.stream_opened_at IS NULL OR $11::timestamptz > c.stream_opened_at))
         OR ($12::uuid IS NOT NULL AND c.local_session_uuid IS NULL)
         OR (c.session_id IS NULL
             AND EXISTS (SELECT 1 FROM sessions.sessions s
                          WHERE s.session_uuid = COALESCE(c.local_session_uuid, $12::uuid)))
         OR ($16::timestamptz IS NOT NULL
             AND (c.post_turn_summary_at IS NULL OR $16::timestamptz >= c.post_turn_summary_at)
             AND $15::jsonb IS DISTINCT FROM c.post_turn_summary)
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
    SELECT $1::text, $2::text, $3::bigint, $18::timestamptz, $4::timestamptz,
           $5::timestamptz, $6::integer, $7::integer,
           $8::text, $9::timestamptz, $10::timestamptz, $11::timestamptz,
           $12::uuid, $13::text, $14::text, $15::jsonb, $16::timestamptz,
           (SELECT s.id FROM sessions.sessions s WHERE s.session_uuid = $12::uuid)
     WHERE NOT EXISTS (SELECT 1 FROM cur)
    ON CONFLICT (cse_id) DO NOTHING
    RETURNING id, worker_status, worker_status_at
)
SELECT COALESCE((SELECT id FROM upd), (SELECT id FROM ins), (SELECT id FROM cur)) AS id,
       (SELECT worker_status FROM cur) AS previous_status,
       (SELECT worker_status_at FROM cur) AS previous_status_at,
       COALESCE((SELECT worker_status FROM upd), (SELECT worker_status FROM ins),
                (SELECT worker_status FROM cur)) AS current_status,
       COALESCE((SELECT worker_status_at FROM upd), (SELECT worker_status_at FROM ins),
                (SELECT worker_status_at FROM cur)) AS current_status_at
