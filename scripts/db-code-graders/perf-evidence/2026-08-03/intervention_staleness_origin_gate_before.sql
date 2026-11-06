SELECT i.expires_at, (sub.id IS NOT NULL) AS "superseded!"
     FROM steering.interventions i
     LEFT JOIN LATERAL (
         SELECT r.id
           FROM requests.requests r
          WHERE r.session_id = i.session_id
            AND r.id > i.trigger_request_id
            AND r.human_prompt <> ''
            AND r.human_prompt NOT LIKE '<channel %'
          LIMIT 1
     ) sub ON true
    WHERE i.uuid = 'a04fcf4f-37a9-4176-b9fe-6decce3ed906';
