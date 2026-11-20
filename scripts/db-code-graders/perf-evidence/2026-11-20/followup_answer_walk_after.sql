WITH RECURSIVE walk AS (
        (SELECT r.id, r.correction_request_id, 0 AS depth
           FROM responses.responses rr
           JOIN requests.requests r ON r.id = rr.request_id
          WHERE rr.upstream_message_id = 'msg_011ChBjMJ1FMmi4APrkw8rHj'
          ORDER BY rr.request_id DESC
          LIMIT 1)
        UNION ALL
        SELECT r.id, r.correction_request_id, w.depth + 1
          FROM walk w
          JOIN requests.requests r ON r.id = w.correction_request_id
         WHERE w.depth < 8
    )
    SELECT w.depth, rr.upstream_message_id
      FROM walk w
      LEFT JOIN responses.responses rr ON rr.request_id = w.id
     ORDER BY w.depth DESC
     LIMIT 1
