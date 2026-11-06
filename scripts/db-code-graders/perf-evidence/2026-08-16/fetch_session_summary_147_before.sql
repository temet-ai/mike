SELECT s.session_uuid::text AS session_id,
        p.project_path,
        CASE WHEN s.project_resolution = 'unresolved' OR p.project_path = '(no project)' THEN NULL ELSE p.id END AS project_id,
        s.cwd,
        s.project_resolution,
        s.first_seen,
        s.last_seen,
        s.last_status,
        s.title,
        s.pinned,
        p.pinned AS project_pinned,
        COUNT(r.id)::int4 AS request_count,
        COALESCE(
            ARRAY_AGG(DISTINCT r.model ORDER BY r.model)
                FILTER (WHERE r.model IS NOT NULL),
            ARRAY[]::text[]
        ) AS last_models,
        AVG(EXTRACT(EPOCH FROM (r.completed_at - r.created_at)) * 1000.0)
            FILTER (WHERE r.completed_at IS NOT NULL)::float8 AS avg_latency_ms,
        first_op.user_prompt AS originating_operator_prompt,
        sjq.session_outcome,
        sjq.session_quality_score,
        sjq.session_judged_at,
        s.overview,
        s.overview_generated_at,
        mbx.pending_inbox_count,
        COALESCE(
            ARRAY_AGG(DISTINCT rp.project_path ORDER BY rp.project_path)
                FILTER (WHERE rp.project_path IS NOT NULL),
            ARRAY[]::text[]
        ) AS projects_touched
 FROM sessions.sessions s
 JOIN projects.projects p ON p.id = s.project_id
 LEFT JOIN requests.requests r ON r.session_id = s.id
 LEFT JOIN projects.projects rp ON rp.id = r.project_id
 LEFT JOIN LATERAL (
     SELECT COALESCE(ot.user_prompt, sr.human_prompt) AS user_prompt
       FROM prompt_kinds.classifications pk_cls
       JOIN prompt_kinds.kinds pk_kinds
         ON pk_kinds.id = pk_cls.kind_id AND pk_kinds.code = 'operator'
       JOIN memory.operator_turns ot
         ON ot.start_request_id = pk_cls.request_id
       LEFT JOIN requests.requests sr ON sr.id = ot.start_request_id
      WHERE pk_cls.session_id = s.id
      ORDER BY pk_cls.request_id ASC
      LIMIT 1
 ) first_op ON TRUE
 LEFT JOIN LATERAL (
     SELECT o.code AS session_outcome,
            v.quality_score::float8 AS session_quality_score,
            v.judged_at AS session_judged_at
       FROM session_judges.verdicts v
       JOIN session_judges.outcomes o ON o.id = v.outcome_id
      WHERE v.session_id = s.id
      ORDER BY v.judged_at DESC, v.id DESC
      LIMIT 1
 ) sjq ON TRUE
 LEFT JOIN LATERAL (
     SELECT COUNT(*)::int4 AS pending_inbox_count
       FROM mailbox.messages m
      WHERE m.to_session_id = s.id
        AND m.sent_request_id IS NULL
 ) mbx ON TRUE
 WHERE s.id = 100749
 GROUP BY s.id, p.id, first_op.user_prompt,
          sjq.session_outcome, sjq.session_quality_score, sjq.session_judged_at,
          mbx.pending_inbox_count;
