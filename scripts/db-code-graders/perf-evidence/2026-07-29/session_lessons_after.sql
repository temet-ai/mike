SELECT l.id AS lesson_id,
       l.text AS lesson_text,
       lk.code AS kind,
       count(*) AS injection_count,
       min(li.injected_at) AS first_injected_at,
       max(li.injected_at) AS last_injected_at
  FROM memory.lesson_injections li
  JOIN memory.lessons l ON l.id = li.lesson_id
  JOIN memory.lesson_kinds lk ON lk.id = l.kind_id
 WHERE li.session_id = 600894
 GROUP BY l.id, l.text, lk.code
 ORDER BY l.id
