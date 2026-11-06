SELECT s.id AS session_id, s.last_seen,
        p.project_path,
        g.role,
        EXISTS (SELECT 1 FROM launch.templates lt WHERE lt.session_uuid = s.session_uuid)
            AS is_template_session,
        fp.human_prompt AS first_prompt,
        COALESCE(lanes.primary_without_agent_id, 0) AS primary_without_agent_id,
        COALESCE(lanes.agent_id_requests, 0) AS agent_id_requests,
        COALESCE(lanes.workflow_agent_requests, 0) AS workflow_agent_requests,
        COALESCE(launches.items, '[]'::jsonb) AS launches
   FROM sessions.sessions s
   JOIN projects.projects p ON p.id = s.project_id
   LEFT JOIN sessions.session_goals g ON g.session_id = s.id
   LEFT JOIN LATERAL (
        SELECT r.human_prompt
          FROM requests.requests r
          JOIN requests.turn_types tt ON tt.id = r.turn_type_id
          JOIN requests.turn_origins o ON o.id = r.turn_origin_id
         WHERE r.session_id = s.id
           AND tt.code = 'user_prompt'
           AND o.code IN ('primary', 'primary_first')
           AND r.human_prompt <> ''
         ORDER BY r.id
         LIMIT 1
   ) fp ON true
   LEFT JOIN (
        SELECT r.session_id,
               count(*) FILTER (WHERE o.code IN ('primary', 'primary_first')
                                  AND r.claude_code_agent_id IS NULL) AS primary_without_agent_id,
               count(*) FILTER (WHERE r.claude_code_agent_id IS NOT NULL) AS agent_id_requests,
               count(*) FILTER (WHERE r.claude_code_agent_id IS NOT NULL
                                  AND o.code = 'workflow_subagent') AS workflow_agent_requests
          FROM requests.requests r
          JOIN requests.turn_origins o ON o.id = r.turn_origin_id
         WHERE r.session_id = ANY(ARRAY[1222224,1221531,1222035,1224139,1221869,1217545,1203246,1223097,1216701,1220495,1221723,1202255,1218515,1220025,1216426,1215649,1215655,1203180,1202152,1216772,1215176,1214695,1213893,1212033,1207103]::bigint[])
         GROUP BY r.session_id
   ) lanes ON lanes.session_id = s.id
   LEFT JOIN (
        SELECT l.session_uuid,
               jsonb_agg(jsonb_build_object(
                   'launch_id', l.id,
                   'run_id', l.run_id,
                   'seat', l.seat,
                   'template_id', l.template_id,
                   'fork', COALESCE(l.claude_args @> '["--fork-session"]'::jsonb, false)
               ) ORDER BY l.id) AS items
          FROM launch.launches l
         WHERE l.session_uuid IS NOT NULL
         GROUP BY l.session_uuid
   ) launches ON launches.session_uuid = s.session_uuid
  WHERE s.id = ANY(ARRAY[1222224,1221531,1222035,1224139,1221869,1217545,1203246,1223097,1216701,1220495,1221723,1202255,1218515,1220025,1216426,1215649,1215655,1203180,1202152,1216772,1215176,1214695,1213893,1212033,1207103]::bigint[])
  ORDER BY s.id
