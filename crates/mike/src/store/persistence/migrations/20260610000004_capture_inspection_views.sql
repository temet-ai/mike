-- Capture-inspection views for the proxy-q CLI (catalogue §1, 2026-06-10).
--
-- Four read-side projections over the squashed lookup-table schema so that
-- ad-hoc inspection queries stop re-deriving the same joins by hand:
--   requests.requests_v       — requests + lookup codes + session_uuid + project_path
--   requests.request_bodies_v — full request body as jsonb + full system text
--   memory.operator_turns_v   — operator turns + session/project identity + start-request context
--   tools.uses_v              — tool uses + request/session/project context + command
--
-- Expand-only: CREATE OR REPLACE VIEW, no table changes. All values are FULL
-- (inspection-fidelity.md): no slicing of any kind — persona_text and
-- system_text are the complete block contents.

-- ---------------------------------------------------------------------------
-- requests.requests_v
--
-- Every scalar column of requests.requests with the four lookup FKs resolved
-- to their codes, plus session_uuid and project_path. The two bytea body
-- columns (request_body, filtered_request_body) are intentionally not
-- projected here — the full body is reachable from the same call site via
-- requests.request_bodies_v keyed on the same request id (full-content path,
-- not a truncation: nothing here is a slice of them).
-- ---------------------------------------------------------------------------
CREATE OR REPLACE VIEW requests.requests_v AS
SELECT
    r.id,
    r.session_id,
    s.session_uuid,
    r.project_id,
    p.project_path,
    m.code  AS method,
    r.path,
    r.model,
    r.is_streaming,
    r.message_count,
    r.is_tool_loop,
    st.code AS status,
    tt.code AS turn_type,
    tor.code AS turn_origin,
    r.subagent_persona,
    r.last_user_message,
    r.human_prompt,
    r.params,
    r.request_headers,
    r.intercepted,
    r.correction_request_id,
    r.agent_instance_id,
    r.operator_turn_id,
    r.created_at,
    r.completed_at,
    r.persisted_at
FROM requests.requests r
JOIN sessions.sessions    s   ON s.id   = r.session_id
JOIN projects.projects    p   ON p.id   = r.project_id
JOIN requests.methods     m   ON m.id   = r.method_id
JOIN requests.statuses    st  ON st.id  = r.status_id
JOIN requests.turn_types  tt  ON tt.id  = r.turn_type_id
JOIN requests.turn_origins tor ON tor.id = r.turn_origin_id;

COMMENT ON VIEW requests.requests_v IS
    'requests.requests with method/status/turn_type/turn_origin codes resolved, plus session_uuid and project_path. Full bodies: requests.request_bodies_v.';

-- ---------------------------------------------------------------------------
-- requests.request_bodies_v
--
-- The captured wire body decoded to jsonb, with the FULL system-prompt text
-- materialised per row. Only rows that actually carry a body appear here
-- (request_body IS NULL rows have nothing to decode).
--
--   body               — convert_from(request_body,'UTF8')::jsonb (full wire payload)
--   is_subagent        — billing marker 'cc_is_subagent=true' anywhere in the raw body text
--   system_text        — full text of ALL system blocks, '\n\n'-joined in wire order
--                        (or the bare string when the API shape is a plain string)
--   system_len         — char length of system_text
--   system_block_count — number of system blocks (1 for the plain-string shape)
--   persona_text       — FULL text of system block index 2 (the persona block);
--                        NULL when the system array has fewer than 3 blocks
-- ---------------------------------------------------------------------------
CREATE OR REPLACE VIEW requests.request_bodies_v AS
SELECT
    r.id AS request_id,
    r.model,
    r.created_at,
    raw.body,
    raw.body_text LIKE '%cc_is_subagent=true%' AS is_subagent,
    sys.system_text,
    length(sys.system_text) AS system_len,
    sys.system_block_count,
    CASE WHEN jsonb_typeof(raw.body -> 'system') = 'array'
         THEN raw.body -> 'system' -> 2 ->> 'text'
    END AS persona_text
FROM requests.requests r
CROSS JOIN LATERAL (
    SELECT bt AS body_text, bt::jsonb AS body
    FROM convert_from(r.request_body, 'UTF8') AS bt
) raw
LEFT JOIN LATERAL (
    SELECT
        CASE jsonb_typeof(raw.body -> 'system')
            WHEN 'array' THEN (
                SELECT string_agg(blk.value ->> 'text', E'\n\n' ORDER BY blk.ordinality)
                FROM jsonb_array_elements(raw.body -> 'system')
                     WITH ORDINALITY AS blk(value, ordinality)
            )
            WHEN 'string' THEN raw.body #>> '{system}'
        END AS system_text,
        CASE jsonb_typeof(raw.body -> 'system')
            WHEN 'array'  THEN jsonb_array_length(raw.body -> 'system')
            WHEN 'string' THEN 1
        END AS system_block_count
) sys ON true
WHERE r.request_body IS NOT NULL;

COMMENT ON VIEW requests.request_bodies_v IS
    'Full captured request body as jsonb plus full system text / persona block (system block index 2). Full values only — never sliced.';

-- ---------------------------------------------------------------------------
-- memory.operator_turns_v
--
-- operator_turns with outcome code resolved, session/project identity, the
-- char length of the (full, also-projected) user_prompt, and the start
-- request's human_prompt + turn_origin for orientation.
-- ---------------------------------------------------------------------------
CREATE OR REPLACE VIEW memory.operator_turns_v AS
SELECT
    ot.id,
    ot.session_id,
    s.session_uuid,
    ot.project_id,
    p.project_path,
    ot.start_request_id,
    ot.end_request_id,
    ot.outcome_evidence_request_id,
    oc.code AS outcome,
    ot.user_prompt,
    length(ot.user_prompt) AS prompt_len,
    sr.human_prompt AS start_human_prompt,
    sto.code        AS start_turn_origin,
    ot.final_assistant_text,
    ot.tool_call_count,
    ot.tool_error_count,
    ot.weasel_findings_count,
    ot.total_input_tokens,
    ot.total_output_tokens,
    ot.tools_used,
    ot.agents_dispatched,
    ot.files_touched,
    ot.classified_at,
    ot.created_at,
    ot.closed_at,
    ot.embedded_at
FROM memory.operator_turns ot
JOIN sessions.sessions      s   ON s.id   = ot.session_id
JOIN projects.projects      p   ON p.id   = ot.project_id
JOIN memory.turn_outcomes   oc  ON oc.id  = ot.outcome_id
JOIN requests.requests      sr  ON sr.id  = ot.start_request_id
JOIN requests.turn_origins  sto ON sto.id = sr.turn_origin_id;

COMMENT ON VIEW memory.operator_turns_v IS
    'memory.operator_turns with outcome code, session_uuid, project_path, prompt_len, and the start request''s human_prompt + turn_origin.';

-- ---------------------------------------------------------------------------
-- tools.uses_v
--
-- tools.uses with request/session/project context. is_subagent / persona_text
-- come from requests.request_bodies_v (LEFT JOIN: NULL when the request row
-- carries no captured body — honest "unknown", not false).
-- ---------------------------------------------------------------------------
CREATE OR REPLACE VIEW tools.uses_v AS
SELECT
    u.id,
    u.wire_tool_use_id,
    u.request_id,
    u.ord,
    u.tool_name,
    u.input,
    u.input ->> 'command' AS command,
    u.file_path,
    u.operation,
    u.created_at,
    r.created_at AS request_created_at,
    s.session_uuid,
    p.project_path,
    rb.is_subagent,
    rb.persona_text
FROM tools.uses u
JOIN requests.requests  r ON r.id = u.request_id
JOIN sessions.sessions  s ON s.id = r.session_id
JOIN projects.projects  p ON p.id = r.project_id
LEFT JOIN requests.request_bodies_v rb ON rb.request_id = u.request_id;

COMMENT ON VIEW tools.uses_v IS
    'tools.uses with command (input->>''command''), session_uuid, project_path, and is_subagent/persona_text from requests.request_bodies_v.';
