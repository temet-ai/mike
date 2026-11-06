WITH latest AS (
    SELECT sp.id
      FROM requests.requests q
      JOIN sessions.sessions s ON s.id = q.session_id
      JOIN requests.turn_origins o ON o.id = q.turn_origin_id
      JOIN requests.system_prompts sp ON sp.request_id = q.id
     WHERE s.session_uuid = '0d7708f6-0cb9-4851-a953-e0553f75011f'
       AND o.code IN ('primary', 'primary_first')
     ORDER BY q.created_at DESC, q.id DESC
     LIMIT 1
)
SELECT count(f.id) > 0 AS filtered
  FROM latest
LEFT JOIN requests.system_prompt_firing_rules f
       ON f.request_system_prompt_id = latest.id
