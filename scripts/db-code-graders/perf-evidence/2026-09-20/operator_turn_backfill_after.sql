-- Capture executes this query with SET LOCAL jit = off.
WITH RECURSIVE parents AS NOT MATERIALIZED (
    SELECT r.id AS request_id, u.request_id AS parent_request_id
FROM requests.requests r
JOIN agents.instances i ON i.id = r.agent_instance_id
JOIN agents.dispatches d ON d.id = i.dispatch_id
JOIN tools.uses u ON u.id = d.tool_use_id
UNION ALL
SELECT r.id, u.request_id
FROM agents.instances i
JOIN agents.dispatches d ON d.id = i.dispatch_id
JOIN tools.uses u ON u.id = d.tool_use_id
JOIN requests.requests parent ON parent.id = u.request_id
JOIN requests.requests r ON r.session_id = i.session_id
WHERE i.session_id <> parent.session_id
UNION ALL
SELECT r.id, u.request_id
FROM agents.dispatches d
JOIN tools.uses u ON u.id = d.tool_use_id
JOIN requests.requests parent ON parent.id = u.request_id
JOIN requests.requests r ON r.session_id = parent.session_id
WHERE d.worker_producer_id IS NOT NULL
  AND (jsonb_path_query_first(r.request_headers, 'strict $[*] ? (@[0] == "x-claude-code-agent-id")[1]', '{}'::jsonb, true) #>> '{}') = d.worker_producer_id
UNION ALL
SELECT r.id, spawn.request_id
FROM (
    SELECT u.request_id, result.content
    FROM tools.uses u JOIN tools.results result ON result.tool_use_id = u.id AND NOT result.is_error
    WHERE u.tool_name IN ('spawn_session', 'mcp__mike__spawn_session')
) spawn
-- Captured MCP results may be structured objects, JSON strings or text blocks.
CROSS JOIN LATERAL jsonb_array_elements(
    CASE WHEN jsonb_typeof(spawn.content) = 'array' THEN spawn.content
         ELSE jsonb_build_array(spawn.content) END
) block(value)
CROSS JOIN LATERAL (
    SELECT CASE
        WHEN block.value->>'type' = 'text' AND pg_input_is_valid(block.value->>'text', 'jsonb')
            THEN (block.value->>'text')::jsonb
        WHEN jsonb_typeof(block.value) = 'string' AND pg_input_is_valid(block.value #>> '{}', 'jsonb')
            THEN (block.value #>> '{}')::jsonb
        ELSE block.value END AS value
) payload
JOIN sessions.sessions s ON s.session_uuid::text = payload.value #>> '{launch,session}'
JOIN requests.requests r ON r.session_id = s.id
), targets AS (
    SELECT r.id
    FROM requests.requests r
    JOIN requests.turn_origins o ON o.id = r.turn_origin_id
    -- Aggregate the parameterized lookup so the planner cannot expand the
    -- entire dispatch graph merely to check a single request's eligibility.
    CROSS JOIN LATERAL (
        SELECT count(*) AS count FROM parents WHERE request_id = r.id
    ) links
    WHERE r.operator_turn_id IS NULL
      AND r.id = ANY('{411639,411632,411621,411620,411488,411472,411462,411445,411434,411395,411388,411373,411371,411364,411353,411338,411304,411291,411277,411273,411271,411269,411266,411265,411263,411257,411199,411122,411115,411026,411025,411024,411023,411022,411021,411020,411019,411018,411017,411016,411015,411014,411013,411012,411011,411010,411009,411008,411007,411006,411005,411004,411003,411002,411001,411000,410999,410998,410997,410996,410994,410993,410992,410991,410990,410989,410986,410985,410984,410981,410979,410973,410969,410963,410960,410957,410955,410953,410948,410940,410836,410825,410812,410792,410742,410736,410697,410688,410680,410660,410643,410620,410607,410555,410538,410536,410535,410518,410484,410465}'::bigint[])
      AND (o.code = 'subagent' OR links.count > 0)
), ancestry AS (
    SELECT id AS request_id, id AS ancestor_id FROM targets
    UNION
    SELECT a.request_id, p.parent_request_id
    FROM ancestry a
    -- A recursive frontier is small; keep parent lookups parameterized by its
    -- request ids instead of materializing every worker request in the store.
    JOIN LATERAL (
        SELECT min(parent_request_id) AS parent_request_id
        FROM parents WHERE request_id = a.ancestor_id
        HAVING count(DISTINCT parent_request_id) = 1
    ) p ON true
), temporal AS (
    SELECT DISTINCT ON (r.id) r.id, t.id AS turn_id
    FROM requests.requests r
    JOIN (SELECT DISTINCT ancestor_id FROM ancestry) a ON a.ancestor_id = r.id
    JOIN memory.operator_turns t ON t.session_id = r.session_id
    JOIN requests.requests start ON start.id = t.start_request_id
    -- A leaf lookup uses the same indexed request boundary as recursion.
    CROSS JOIN LATERAL (
        SELECT count(*) AS count FROM parents WHERE request_id = r.id
    ) links
    WHERE r.operator_turn_id IS NULL
      AND (start.created_at, start.id) <= (r.created_at, r.id)
      AND links.count = 0
    ORDER BY r.id, start.created_at DESC, start.id DESC
), choices AS (
    SELECT a.request_id, r.operator_turn_id AS turn_id
    FROM ancestry a
    JOIN requests.requests r ON r.id = a.ancestor_id
    WHERE r.operator_turn_id IS NOT NULL
    UNION ALL
    SELECT a.request_id, t.turn_id FROM ancestry a JOIN temporal t ON t.id = a.ancestor_id
), resolved AS (
    SELECT request_id, min(turn_id) AS turn_id FROM choices
    GROUP BY request_id HAVING count(DISTINCT turn_id) = 1
), stamped AS (
    UPDATE requests.requests r SET operator_turn_id = resolved.turn_id
    FROM resolved WHERE r.id = resolved.request_id AND r.operator_turn_id IS NULL
    RETURNING r.operator_turn_id
)
SELECT DISTINCT operator_turn_id FROM stamped;
