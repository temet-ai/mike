INSERT INTO memory.lesson_injections
    (lesson_id, request_id, session_id, lane_id, origin)
SELECT l.id, 648335, s.id, lane.id, 'mod'
  FROM UNNEST(ARRAY[1, 15, 16, 17, 18]::bigint[]) AS u(lesson_id)
  JOIN memory.lessons l ON l.id = u.lesson_id
  JOIN sessions.sessions s ON s.session_uuid = 'bb99ed66-72e1-4eec-a381-7434993dfb56'::text::uuid
  JOIN memory.lesson_injection_lanes lane ON lane.code = 'primary'
 WHERE NOT EXISTS (
       SELECT 1 FROM memory.lesson_injections seen
        WHERE seen.session_id = s.id
          AND seen.lesson_id = l.id
          AND seen.origin = 'mod')
ON CONFLICT (lesson_id, request_id) DO NOTHING
