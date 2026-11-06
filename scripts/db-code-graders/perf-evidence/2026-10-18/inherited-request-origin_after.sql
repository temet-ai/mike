SELECT r.operator_turn_id FROM requests.requests r
         JOIN memory.operator_turns t ON t.id = r.operator_turn_id
         JOIN requests.turn_origins o ON o.id = r.turn_origin_id
         WHERE r.id = 662404 AND o.code = 'subagent'