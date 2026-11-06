WITH RECURSIVE parents AS NOT MATERIALIZED (
    SELECT * FROM memory.request_dispatch_parents
), targets AS (
    SELECT r.id, r.session_id
    FROM requests.requests r
    JOIN requests.turn_origins o ON o.id = r.turn_origin_id
    WHERE r.operator_turn_id IS NULL
      AND r.id = ANY($1::bigint[])
      AND o.code = 'subagent'
), ancestry AS (
    SELECT id AS request_id, id AS ancestor_id, session_id FROM targets
    UNION
    SELECT a.request_id, p.parent_request_id, a.session_id
    FROM ancestry a
    -- A recursive frontier is small; keep parent lookups parameterized by its
    -- request ids instead of materializing every worker request in the store.
    JOIN LATERAL (
        SELECT min(parent_request_id) AS parent_request_id
        FROM parents WHERE request_id = a.ancestor_id
        HAVING count(DISTINCT parent_request_id) = 1
    ) p ON true
), temporal AS (
    SELECT DISTINCT ON (r.id) r.id, r.session_id, t.id AS turn_id
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
    -- A request takes a turn only from its own session (#619): an ancestor in
    -- another session offers none, so a spawned session never inherits.
    SELECT a.request_id, r.operator_turn_id AS turn_id
    FROM ancestry a
    JOIN requests.requests r ON r.id = a.ancestor_id
    WHERE r.operator_turn_id IS NOT NULL
      AND r.session_id = a.session_id
    UNION ALL
    SELECT a.request_id, t.turn_id FROM ancestry a
    JOIN temporal t ON t.id = a.ancestor_id AND t.session_id = a.session_id
), resolved AS (
    SELECT request_id, min(turn_id) AS turn_id FROM choices
    GROUP BY request_id HAVING count(DISTINCT turn_id) = 1
), stamped AS (
    UPDATE requests.requests r SET operator_turn_id = resolved.turn_id
    FROM resolved WHERE r.id = resolved.request_id AND r.operator_turn_id IS NULL
    RETURNING r.operator_turn_id
)
SELECT DISTINCT operator_turn_id FROM stamped;
