-- Every call of one channel's batch, with its kept payloads and its
-- worker-status transitions, in one set-based statement, bound from
-- call_batch.rs. It runs after touch_channel.sql has rolled the whole batch up
-- into the channel row, inside the same transaction.
--
-- Call ids are drawn from the identity sequence up front so payloads,
-- preserved event ids and transitions can join to their call by its 1-based
-- position in the batch (ord) instead of re-deriving it from content.
--
-- Bind parameters:
--   $1        channel id
--   $2-$18    one array per call column, in batch order (method code, path,
--             http_status, request/response body + sha256 + size, created_at,
--             completed_at, payload tallies, is_compaction, session_agent_id,
--             whether preserved_event_ids was present)
--   $19-$20   preserved event ids flattened as (ord, event_id)
--   $21-$24   transitions as (ord, from_status, to_status, observed_at)
--   $25-$30   kept payloads as (ord, ordinal, type, body, sha, size)
WITH
new_calls AS MATERIALIZED (
    SELECT u.ord::int AS ord,
           nextval(pg_get_serial_sequence('remote_control.calls', 'id')) AS id,
           u.method, u.path, u.http_status,
           u.request_body, u.request_sha, u.request_size,
           u.response_body, u.response_sha, u.response_size,
           u.created_at, u.completed_at,
           u.payload_count, u.kept_count, u.type_counts,
           u.is_compaction, u.session_agent_id, u.has_preserved
      FROM UNNEST($2::text[], $3::text[], $4::int[],
                  $5::bytea[], $6::bytea[], $7::int[],
                  $8::bytea[], $9::bytea[], $10::int[],
                  $11::timestamptz[], $12::timestamptz[],
                  $13::int[], $14::int[], $15::jsonb[],
                  $16::boolean[], $17::text[], $18::boolean[])
           WITH ORDINALITY
           AS u(method, path, http_status,
                request_body, request_sha, request_size,
                response_body, response_sha, response_size,
                created_at, completed_at,
                payload_count, kept_count, type_counts,
                is_compaction, session_agent_id, has_preserved, ord)
),
preserved AS (
    SELECT p.ord, array_agg(p.event_id ORDER BY p.pos) AS ids
      FROM UNNEST($19::int[], $20::text[]) WITH ORDINALITY AS p(ord, event_id, pos)
     GROUP BY p.ord
),
call_rows AS (
    INSERT INTO remote_control.calls
        (id, channel_id, method_id, path, http_status,
         request_body, request_body_sha256, request_body_size,
         response_body, response_body_sha256, response_body_size,
         created_at, completed_at,
         payload_count, payload_kept_count, payload_type_counts,
         is_compaction, session_agent_id, preserved_event_ids)
    OVERRIDING SYSTEM VALUE
    SELECT n.id, $1::bigint, rm.id, n.path, n.http_status,
           n.request_body, n.request_sha, n.request_size,
           n.response_body, n.response_sha, n.response_size,
           n.created_at, n.completed_at,
           n.payload_count, n.kept_count, n.type_counts,
           n.is_compaction, n.session_agent_id,
           CASE WHEN n.has_preserved THEN COALESCE(pr.ids, '{}'::text[]) END
      FROM new_calls n
      JOIN requests.methods rm ON rm.code = n.method
      LEFT JOIN preserved pr ON pr.ord = n.ord
    RETURNING id
),
transitions AS (
    INSERT INTO remote_control.worker_status_transitions
        (channel_id, from_status, to_status, observed_at, call_id)
    SELECT $1::bigint, t.from_status, t.to_status, t.observed_at, n.id
      FROM UNNEST($21::int[], $22::text[], $23::text[], $24::timestamptz[])
           AS t(ord, from_status, to_status, observed_at)
      JOIN new_calls n ON n.ord = t.ord
    RETURNING 1
),
kept_payloads AS (
    INSERT INTO remote_control.call_payloads
        (call_id, ordinal, payload_type, body, body_sha256, body_size, created_at)
    SELECT n.id, p.ordinal, p.payload_type, p.body, p.sha, p.size, n.created_at
      FROM UNNEST($25::int[], $26::int[], $27::text[], $28::bytea[],
                  $29::bytea[], $30::int[])
           AS p(ord, ordinal, payload_type, body, sha, size)
      JOIN new_calls n ON n.ord = p.ord
    RETURNING 1
)
SELECT c.id AS "id!"
  FROM call_rows c
  JOIN new_calls n ON n.id = c.id
 ORDER BY n.ord
