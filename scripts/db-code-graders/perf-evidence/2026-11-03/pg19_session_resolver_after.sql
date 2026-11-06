INSERT INTO sessions.sessions
                         (session_uuid, project_id, first_seen, last_seen,
                          flow_capture_version, flow_admitted_at, flow_producer,
                          flow_hook_event_name, flow_start_source)
                     VALUES ('b411e922-620b-40e2-8a36-e9536b92b0ae'::text::uuid, 993478, NOW(), NOW(),
                         CASE WHEN false THEN 1 END,
                         CASE WHEN false THEN clock_timestamp() END,
                         CASE WHEN false THEN NULL::text END,
                         CASE WHEN false THEN NULL::text END,
                         CASE WHEN false THEN NULL::text END)
                     ON CONFLICT (session_uuid) DO SELECT
                     RETURNING id AS "id!", project_id AS "project_id!"
