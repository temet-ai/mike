SELECT li.id, li.lesson_id, li.request_id, li.session_id,
       s.session_uuid::text AS session_uuid, li.injected_at,
       lane.code AS lane, li.persona
  FROM memory.lesson_injections li
  JOIN memory.lesson_injection_lanes lane ON lane.id = li.lane_id
  LEFT JOIN sessions.sessions s ON s.id = li.session_id
 WHERE li.lesson_id = 1
 ORDER BY li.injected_at ASC
