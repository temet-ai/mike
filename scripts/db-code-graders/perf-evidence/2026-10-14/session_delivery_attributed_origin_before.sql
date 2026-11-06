SELECT count(DISTINCT li.request_id) AS count
  FROM memory.lesson_injections li
 WHERE li.session_id = 1504120
