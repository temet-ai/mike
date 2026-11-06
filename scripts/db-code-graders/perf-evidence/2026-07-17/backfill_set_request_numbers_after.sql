UPDATE requests.requests r
    SET request_number = n.rn
   FROM (
            SELECT id, ROW_NUMBER() OVER (
                       PARTITION BY session_id ORDER BY created_at ASC, id ASC
                   ) AS rn
              FROM requests.requests
        ) n
  WHERE n.id = r.id
    AND r.request_number IS DISTINCT FROM n.rn;
