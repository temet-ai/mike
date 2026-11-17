INSERT INTO prompt_cache.breaks (
    request_id, session_id, prev_request_id, source_request_id, kind, cause, stage, location,
    woken_by, ttl_seconds, gap_seconds, expected_read, cache_read, tokens_rewritten, detail,
    occurred_at
)
SELECT u.request_id, u.session_id, u.prev_request_id, u.source_request_id, u.kind, u.cause,
       u.stage, u.location, u.woken_by, u.ttl_seconds, u.gap_seconds, u.expected_read,
       u.cache_read, u.tokens_rewritten, u.detail, u.occurred_at
  FROM unnest(
        $1::bigint[], $2::bigint[], $3::bigint[], $4::bigint[], $5::text[], $6::text[],
        $7::text[], $8::text[], $9::text[], $10::bigint[], $11::bigint[], $12::bigint[],
        $13::bigint[], $14::bigint[], $15::jsonb[], $16::timestamptz[]
       ) AS u(request_id, session_id, prev_request_id, source_request_id, kind, cause, stage,
              location, woken_by, ttl_seconds, gap_seconds, expected_read, cache_read,
              tokens_rewritten, detail, occurred_at)
ON CONFLICT (request_id) DO NOTHING
