SELECT EXISTS (
  SELECT 1
    FROM requests.requests r
    JOIN sessions.sessions s ON s.id = r.session_id
   WHERE s.session_uuid = $1
     AND r.created_at >= $2
     AND strpos(left(r.human_prompt, length($3) + 64), $3) > 0
) AS "seen!"
