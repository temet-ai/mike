SELECT l.id AS lesson_id,
       l.text,
       lk.code AS kind,
       l.topic,
       ls.code AS stage,
       l.archived_at,
       ar.code AS archived_reason,
       count(*) AS injection_count,
       min(li.injected_at) AS first_injected_at,
       max(li.injected_at) AS last_injected_at,
       min(li.request_id) AS first_request_id,
       max(li.request_id) AS last_request_id,
       array_agg(DISTINCT lane.code) AS lanes,
       array_remove(array_agg(DISTINCT li.persona), NULL) AS personas,
       array_agg(DISTINCT li.origin ORDER BY li.origin) AS origins
  FROM memory.lesson_injections li
  JOIN memory.lessons l ON l.id = li.lesson_id
  JOIN memory.lesson_kinds lk ON lk.id = l.kind_id
  JOIN memory.lesson_stages ls ON ls.id = l.stage_id
  JOIN memory.lesson_injection_lanes lane ON lane.id = li.lane_id
  LEFT JOIN memory.lesson_archived_reasons ar ON ar.id = l.archived_reason_id
 WHERE li.session_id = 1504120
 GROUP BY l.id, l.text, lk.code, l.topic, ls.code, l.archived_at, ar.code
 ORDER BY min(li.injected_at), l.id
