SELECT t.user_prompt
     FROM memory.operator_turns t
     JOIN sessions.sessions s ON s.id = t.session_id
    WHERE s.session_uuid = '12174fed-3435-4866-a4d1-b709e9143fd5' AND t.closed_at IS NULL
    ORDER BY t.id DESC
    LIMIT 1;
