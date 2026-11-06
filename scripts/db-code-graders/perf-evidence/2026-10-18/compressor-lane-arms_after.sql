
            SELECT q.session_id, min(st->'resolved_parameters'->>'arm') AS arm,
                   CASE WHEN min(st->'resolved_parameters'->>'arm') = max(st->'resolved_parameters'->>'arm')
                        THEN 1 ELSE 2 END AS observed_arms
            FROM requests.requests q
            CROSS JOIN LATERAL jsonb_array_elements(q.transformation_manifest->'stages') st
            WHERE q.created_at >= '2026-10-12T00:00:00+00:00' AND q.created_at < '2026-10-13T00:00:00+00:00'
              AND q.transformation_manifest IS NOT NULL
              AND st->>'stage' = 'file_read_compression'
              AND st->'resolved_parameters' ? 'arm'
            GROUP BY q.session_id