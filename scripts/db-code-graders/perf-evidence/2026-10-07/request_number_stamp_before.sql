UPDATE requests.requests
   SET predecessor_request_id = NULL,
       request_number = (
           SELECT COUNT(*)
             FROM requests.requests r2
            WHERE r2.session_id = 981652
              AND (r2.created_at, r2.id) <= ('2026-09-06 08:35:19.317943+00'::timestamptz, 372164::bigint)
       )
 WHERE id = 372164
