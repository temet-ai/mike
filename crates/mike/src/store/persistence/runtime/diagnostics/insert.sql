INSERT INTO runtime.responses_exchanges
    (session_id, provider_session_id, provider_thread_id, provider_turn_id, request_kind,
     correlation, auth_route, transport, method, path, upstream_url, request_headers,
     forwarded_headers, request_body, forwarded_body, request_body_encoding, model,
     response_status, response_headers, response_body, upstream_request_id, response_id,
     response_model, response_state, usage, stream_events, error, status, created_at,
     completed_at, evidence_status, expires_at, metadata_expires_at)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17,
        $18, $19, $20, $21, $22, $23, $24, $25, $26, $27, $28, $29, now(), $30,
        now() + make_interval(secs => $31::double precision), now() + interval '7 days')
RETURNING id
