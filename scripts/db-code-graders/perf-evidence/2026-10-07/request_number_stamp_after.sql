UPDATE requests.requests r
   SET predecessor_request_id = NULL,
       request_number = COALESCE(r.request_number, n.next_number)
  FROM (
       SELECT COALESCE(MAX(r2.request_number), 0) + 1 AS next_number
         FROM requests.requests r2
        WHERE r2.session_id = 981652
  ) n
 WHERE r.id = 372164
