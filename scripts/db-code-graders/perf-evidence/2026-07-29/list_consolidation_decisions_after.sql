SELECT a.id, a.run_id, rk.code AS run_kind,
    a.lesson_id, a.lesson_uuid::text AS lesson_uuid, a.project_id, p.project_path,
    a.session_id, a.session_uuid::text AS session_uuid, a.source_turn_id,
    a.files_touched, a.agent_id, a.lesson_text,
    ak.code AS action_kind, oc.code AS outcome, a.error,
    bs.code AS before_stage, as_.code AS after_stage, db.code AS decided_by,
    a.distance, a.threshold, a.embedding_model,
    a.counterpart_lesson_id, a.counterpart_uuid::text AS counterpart_uuid, a.counterpart_text,
    a.runners_up, a.rank_detail, a.injection_count, a.created_at
   FROM memory.consolidation_actions a
    JOIN memory.consolidation_runs r ON r.id = a.run_id
    JOIN memory.consolidation_run_kinds rk ON rk.id = r.run_kind_id
    LEFT JOIN projects.projects p ON p.id = a.project_id
    JOIN memory.consolidation_action_kinds ak ON ak.id = a.action_kind_id
    JOIN memory.consolidation_outcomes oc ON oc.id = a.outcome_id
    LEFT JOIN memory.lesson_stages bs ON bs.id = a.before_stage_id
    LEFT JOIN memory.lesson_stages as_ ON as_.id = a.after_stage_id
    LEFT JOIN memory.consolidation_decided_by db ON db.id = a.decided_by_id
  WHERE (NULL::text IS NULL OR p.project_path = NULL::text)
    AND (NULL::uuid IS NULL OR a.session_uuid = NULL::uuid)
    AND (NULL::text IS NULL OR a.files_touched && ARRAY[NULL::text])
    AND (NULL::text IS NULL OR ak.code = NULL::text)
  ORDER BY a.created_at DESC
  OFFSET 0 LIMIT 50
