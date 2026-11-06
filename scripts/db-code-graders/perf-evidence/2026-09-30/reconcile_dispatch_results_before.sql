SELECT DISTINCT p.request_id FROM tools.uses u
 JOIN memory.request_dispatch_parents p ON p.parent_request_id = u.request_id
 JOIN requests.requests r ON r.id = p.request_id AND r.operator_turn_id IS NULL
 WHERE u.wire_tool_use_id = ANY(ARRAY['toolu_01RYyf7vSsRXxxgRXUe8PEVU','toolu_011aqtifMg3zJJx3jKzKgVgZ','toolu_01MnvafLXVxcuKrmmoJn6x7x'])
   AND u.tool_name IN ('Agent', 'Task', 'spawn_session', 'mcp__mike__spawn_session')
