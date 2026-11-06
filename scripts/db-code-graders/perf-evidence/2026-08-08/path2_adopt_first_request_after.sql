UPDATE agents.instances
   SET first_request_id = 248253,
       started_at = (SELECT created_at FROM requests.requests WHERE id = 248253)
 WHERE id = 1495
   AND first_request_id IS NULL
