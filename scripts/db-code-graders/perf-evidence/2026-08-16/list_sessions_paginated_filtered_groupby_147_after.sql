WITH ranked AS (
    SELECT s.id,
           s.session_uuid::text AS session_id,
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
           s.overview,
           s.overview_generated_at,
           COUNT(r.id)::int4 AS request_count,
           ARRAY_AGG(DISTINCT r.project_id) AS distinct_project_ids
    FROM sessions.sessions s
    JOIN projects.projects p ON p.id = s.project_id
    LEFT JOIN requests.requests r ON r.session_id = s.id
    WHERE (NULL::timestamptz IS NULL OR s.last_seen >= NULL::timestamptz)
      AND (NULL::timestamptz IS NULL OR s.last_seen < NULL::timestamptz)
      AND (NULL::text IS NULL
           OR s.session_uuid::text = NULL::text
           OR s.title % NULL::text
           OR s.title ILIKE '%' || NULL::text || '%')
      AND (NULL::text IS NULL OR p.project_path ILIKE '%' || NULL::text || '%')
    GROUP BY s.id, p.id
   HAVING (10::bigint IS NULL OR COUNT(r.id) >= 10)
    ORDER BY s.last_seen DESC
    LIMIT 50 OFFSET 0
)
SELECT ranked.session_id, ranked.project_path, ranked.project_id, ranked.cwd, ranked.project_resolution,
       ranked.first_seen, ranked.last_seen, ranked.request_count,
       COALESCE(newest_status.live_status, ranked.last_status) AS last_status,
       ARRAY[]::text[] AS last_models, ranked.title, ranked.pinned, ranked.project_pinned,
       NULL::float8 AS avg_latency_ms, first_op.user_prompt AS originating_operator_prompt,
       sjq.session_outcome, sjq.session_quality_score, sjq.session_judged_at,
       ranked.overview, ranked.overview_generated_at,
       COALESCE(mbx.pending_inbox_count, 0) AS pending_inbox_count, pt.projects_touched
FROM ranked
LEFT JOIN LATERAL (
    SELECT CASE st.code
               WHEN 'pending'   THEN 'Pending'
               WHEN 'streaming' THEN 'Streaming'
               WHEN 'complete'  THEN 'Complete'
               WHEN 'error'     THEN 'Error: ' || COALESCE(re.message, '')
           END AS live_status
      FROM requests.requests nr
      JOIN requests.statuses st ON st.id = nr.status_id
      LEFT JOIN requests.errors re ON re.request_id = nr.id
     WHERE nr.session_id = ranked.id
     ORDER BY nr.created_at DESC, nr.id DESC
     LIMIT 1
) newest_status ON TRUE
LEFT JOIN LATERAL (
    SELECT COALESCE(ot.user_prompt, sr.human_prompt) AS user_prompt
      FROM (
          SELECT pk_cls.request_id FROM prompt_kinds.classifications pk_cls
           WHERE pk_cls.session_id = ranked.id
             AND pk_cls.kind_id = (SELECT id FROM prompt_kinds.kinds WHERE code = 'operator')
           ORDER BY pk_cls.request_id ASC LIMIT 1
      ) first_cls
      JOIN memory.operator_turns ot ON ot.start_request_id = first_cls.request_id
      LEFT JOIN requests.requests sr ON sr.id = first_cls.request_id
) first_op ON TRUE
LEFT JOIN LATERAL (
    SELECT o.code AS session_outcome, v.quality_score::float8 AS session_quality_score, v.judged_at AS session_judged_at
      FROM session_judges.verdicts v JOIN session_judges.outcomes o ON o.id = v.outcome_id
     WHERE v.session_id = ranked.id ORDER BY v.judged_at DESC, v.id DESC LIMIT 1
) sjq ON TRUE
LEFT JOIN (
    SELECT m.to_session_id, COUNT(*)::int4 AS pending_inbox_count
      FROM mailbox.messages m WHERE m.sent_request_id IS NULL GROUP BY m.to_session_id
) mbx ON mbx.to_session_id = ranked.id
LEFT JOIN LATERAL (
    SELECT COALESCE(ARRAY_AGG(pp.project_path ORDER BY pp.project_path), ARRAY[]::text[]) AS projects_touched
      FROM projects.projects pp WHERE pp.id = ANY(ranked.distinct_project_ids)
) pt ON TRUE
ORDER BY ranked.last_seen DESC;
