CREATE OR REPLACE VIEW runtime.model_call_facts AS
SELECT r.id, r.id AS request_id, r.request_number, r.session_id, r.project_id,
       r.model, r.created_at, r.completed_at,
       (EXTRACT(EPOCH FROM (r.completed_at - r.created_at)) * 1000)::double precision AS duration_ms,
       CASE st.code WHEN 'pending' THEN 'Pending' WHEN 'streaming' THEN 'Streaming'
         WHEN 'complete' THEN 'Complete' WHEN 'error' THEN 'Error: ' || COALESCE(er.message, '') END AS live_status,
       rs.response_status, rs.input_tokens::bigint AS input_tokens,
       rs.output_tokens::bigint AS output_tokens,
       rs.cache_read_input_tokens::bigint AS cache_read_input_tokens,
       rs.cache_creation_input_tokens::bigint AS cache_creation_input_tokens,
       r.subagent_persona IS NULL AND origin.code IN ('primary', 'primary_first') AS primary_lane,
       'own'::text AS usage_scope, 'call'::text AS usage_basis,
       (rs.input_tokens::bigint + COALESCE(rs.cache_read_input_tokens, 0)
        + COALESCE(rs.cache_creation_input_tokens, 0))::bigint AS context_tokens
  FROM requests.requests r
  JOIN requests.statuses st ON st.id = r.status_id
  LEFT JOIN requests.turn_origins origin ON origin.id = r.turn_origin_id
  LEFT JOIN LATERAL (SELECT e.message FROM requests.errors e WHERE e.request_id = r.id
                     ORDER BY e.id DESC LIMIT 1) er ON st.code = 'error'
  LEFT JOIN responses.responses rs ON rs.request_id = r.id
 WHERE NOT EXISTS (SELECT 1 FROM runtime.session_facts f WHERE f.session_id = r.session_id)
UNION ALL
SELECT c.id, NULL::bigint, NULL::bigint, c.session_id, s.project_id,
       COALESCE(c.observed_model, c.requested_model), c.observed_at,
       CASE WHEN c.status IN ('complete', 'completed', 'failed', 'error', 'aborted') THEN c.observed_at END,
       c.duration_ms::double precision,
       CASE c.status WHEN 'active' THEN 'Streaming' WHEN 'pending' THEN 'Pending'
         WHEN 'complete' THEN 'Complete' WHEN 'completed' THEN 'Complete'
         WHEN 'failed' THEN 'Error' WHEN 'error' THEN 'Error' WHEN 'aborted' THEN 'Interrupted' END,
       c.http_status, c.input_tokens, c.output_tokens, c.cached_input_tokens, NULL::bigint,
       true, c.usage_scope, c.usage_basis, c.input_tokens
  FROM runtime.model_calls c
  JOIN sessions.sessions s ON s.id = c.session_id
  LEFT JOIN (SELECT DISTINCT session_id FROM runtime.model_calls
             WHERE source = 'responses_wire' AND (input_tokens IS NOT NULL OR output_tokens IS NOT NULL)) wire
    ON wire.session_id = c.session_id
  LEFT JOIN (SELECT DISTINCT session_id FROM runtime.model_calls
             WHERE source = 'otel' AND (input_tokens IS NOT NULL OR output_tokens IS NOT NULL)) otel
    ON otel.session_id = c.session_id
 WHERE (c.source <> 'otel' OR wire.session_id IS NULL)
   AND (c.source <> 'responses_wire' OR wire.session_id IS NOT NULL OR otel.session_id IS NULL);

CREATE OR REPLACE FUNCTION runtime.session_activity(p_session_id bigint)
RETURNS TABLE(observed_at timestamptz, newest_request_number bigint, lifecycle text)
LANGUAGE sql STABLE AS $$
SELECT COALESCE(f.observed_at, latest.created_at), latest.request_number, f.lifecycle
  FROM (VALUES (p_session_id)) session(session_id)
  LEFT JOIN runtime.session_facts f ON f.session_id = session.session_id
  LEFT JOIN LATERAL (
       SELECT r.created_at, r.request_number
         FROM requests.requests r
        WHERE r.session_id = session.session_id AND f.session_id IS NULL
        ORDER BY r.created_at DESC, r.id DESC LIMIT 1
  ) latest ON true
$$;

CREATE OR REPLACE VIEW runtime.session_observation_v AS
SELECT s.id AS session_id, s.session_uuid,
       COALESCE(f.runtime, 'claude') AS runtime,
       COALESCE(f.source, 'wire') AS source,
       f.source_version,
       COALESCE(f.fidelity, 'wire') AS fidelity,
       COALESCE(f.completeness, 'captured') AS completeness,
       COALESCE(f.observed_at, latest.created_at) AS observed_at,
       f.lifecycle, f.lifecycle_at, f.control,
       COALESCE(f.requested_model, latest.model) AS requested_model,
       f.observed_model,
       COALESCE(f.observed_model, f.requested_model, latest.model) AS model,
       CASE f.lifecycle WHEN 'active' THEN 'Streaming' WHEN 'idle' THEN 'Complete'
         WHEN 'ended' THEN 'Complete' WHEN 'failed' THEN 'Error'
         ELSE COALESCE(CASE st.code WHEN 'pending' THEN 'Pending' WHEN 'streaming' THEN 'Streaming'
             WHEN 'complete' THEN 'Complete' WHEN 'error' THEN 'Error: ' || COALESCE(er.message, '') END,
             s.last_status) END AS last_status,
       latest.request_id AS newest_request_id, latest.request_number AS newest_request_number,
       f.usage_scope, f.usage_basis, f.input_tokens, f.cached_input_tokens, f.output_tokens,
       f.reasoning_output_tokens, f.total_tokens, f.context_window
  FROM sessions.sessions s
  LEFT JOIN runtime.session_facts f ON f.session_id = s.id
  LEFT JOIN LATERAL (
       SELECT r.created_at, r.id AS request_id, r.request_number, r.model, r.status_id
         FROM requests.requests r
        WHERE r.session_id = s.id AND f.session_id IS NULL
        ORDER BY r.created_at DESC, r.id DESC LIMIT 1
  ) latest ON true
  LEFT JOIN requests.statuses st ON st.id = latest.status_id
  LEFT JOIN LATERAL (SELECT e.message FROM requests.errors e WHERE e.request_id = latest.request_id
                     ORDER BY e.id DESC LIMIT 1) er ON st.code = 'error';

CREATE OR REPLACE VIEW runtime.session_observation_summary_v AS
SELECT v.session_id, v.session_uuid, v.runtime, v.source, v.source_version, v.fidelity,
       v.completeness, v.observed_at, v.lifecycle, v.lifecycle_at, v.control,
       v.requested_model, v.observed_model, v.model, v.last_status,
       v.newest_request_id, v.newest_request_number,
       CASE WHEN v.usage_basis IS NOT NULL THEN v.usage_scope
            WHEN calls.input_tokens IS NOT NULL OR calls.output_tokens IS NOT NULL THEN 'own' END AS usage_scope,
       CASE WHEN v.usage_basis IS NOT NULL THEN v.usage_basis
            WHEN calls.input_tokens IS NOT NULL OR calls.output_tokens IS NOT NULL THEN 'observed_calls' END AS usage_basis,
       CASE WHEN v.usage_basis IS NOT NULL THEN v.input_tokens ELSE calls.input_tokens END AS input_tokens,
       CASE WHEN v.usage_basis IS NOT NULL THEN v.cached_input_tokens ELSE calls.cached_input_tokens END AS cached_input_tokens,
       CASE WHEN v.usage_basis IS NOT NULL THEN v.output_tokens ELSE calls.output_tokens END AS output_tokens,
       v.reasoning_output_tokens, v.total_tokens, v.context_window,
       calls.model_call_count, calls.avg_latency_ms
  FROM runtime.session_observation_v v
  LEFT JOIN LATERAL (
       SELECT count(*)::bigint AS model_call_count, avg(c.duration_ms)::double precision AS avg_latency_ms,
              sum(c.input_tokens) FILTER (WHERE c.usage_scope = 'own' AND c.usage_basis = 'call')::bigint AS input_tokens,
              sum(c.cache_read_input_tokens) FILTER (WHERE c.usage_scope = 'own' AND c.usage_basis = 'call')::bigint AS cached_input_tokens,
              sum(c.output_tokens) FILTER (WHERE c.usage_scope = 'own' AND c.usage_basis = 'call')::bigint AS output_tokens
         FROM runtime.model_call_facts c WHERE c.session_id = v.session_id
  ) calls ON true;
