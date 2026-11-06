WITH upd AS (
     UPDATE requests.requests r
        SET status_id = error_s.id,
            completed_at = NOW()
       FROM requests.statuses error_s,
            requests.statuses inflight_s
      WHERE error_s.code = 'error'
        AND inflight_s.code IN ('pending', 'streaming')
        AND r.status_id = inflight_s.id
      RETURNING r.id
 )
 INSERT INTO requests.errors (request_id, message)
 SELECT id, 'proxy_crashed_during_stream' FROM upd
