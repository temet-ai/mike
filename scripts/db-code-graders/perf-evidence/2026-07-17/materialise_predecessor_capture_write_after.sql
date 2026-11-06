UPDATE requests.requests
    SET predecessor_request_id = 194380,
        request_number = (
            SELECT COUNT(*)
              FROM requests.requests r2
             WHERE r2.session_id = 542399
               AND (r2.created_at, r2.id) <= ('2026-07-17T15:19:57.970516Z'::timestamptz, 194612::bigint)
        )
  WHERE id = 194612;
