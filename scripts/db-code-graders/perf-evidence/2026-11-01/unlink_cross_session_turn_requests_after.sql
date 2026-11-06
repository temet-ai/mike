UPDATE requests.requests r SET operator_turn_id = NULL FROM memory.operator_turns t WHERE t.id = r.operator_turn_id AND t.session_id <> r.session_id
