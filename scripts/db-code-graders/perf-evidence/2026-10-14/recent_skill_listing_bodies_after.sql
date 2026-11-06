WITH recent AS (
    SELECT r.id, r.session_id, r.created_at, r.request_body
      FROM requests.requests r
     WHERE r.project_id = 993478
       AND r.request_body_size BETWEEN 20000 AND 600000
     ORDER BY r.created_at DESC
     LIMIT 24),
carrying AS (
    SELECT DISTINCT ON (c.session_id) c.id, c.created_at, c.request_body
      FROM recent c
     WHERE position('\x617661696c61626c6520666f722075736520776974682074686520536b696c6c20746f6f6c3a'::bytea IN c.request_body) > 0
     ORDER BY c.session_id, c.created_at DESC)
SELECT k.id AS id, k.request_body AS body
  FROM carrying k
 ORDER BY k.created_at DESC
 LIMIT 5
