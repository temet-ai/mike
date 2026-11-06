SELECT count(DISTINCT n.data->>'event_id') AS observed_goal_limit_stops
FROM workspace.session_notes n
JOIN sessions.sessions s ON s.id = n.session_id
WHERE s.session_uuid IN (SELECT value::uuid FROM jsonb_array_elements_text(:'sessions'::jsonb))
  AND n.created_at >= :'since'::timestamptz AND n.created_at <= :'until'::timestamptz
  AND n.data->>'schema' = 'goal-budget-event-v1'
