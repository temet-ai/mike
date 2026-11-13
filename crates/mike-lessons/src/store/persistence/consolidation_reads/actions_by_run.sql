SELECT a.id AS "id!", a.run_id AS "run_id!", rk.code AS "run_kind!",
       a.lesson_id, a.lesson_uuid::text AS "lesson_uuid!", a.project_id, p.project_path,
       a.session_id, a.session_uuid::text AS "session_uuid?", a.source_turn_id,
       a.files_touched, a.agent_id, a.lesson_text AS "lesson_text!",
       ak.code AS "action_kind!", oc.code AS "outcome!", a.error,
       bs.code AS "before_stage?", as_.code AS "after_stage?", db.code AS "decided_by?",
       a.distance, a.threshold, a.embedding_model,
       a.counterpart_lesson_id, a.counterpart_uuid::text AS "counterpart_uuid?", a.counterpart_text,
       a.runners_up AS "runners_up!: sqlx::types::Json<Vec<super::RunnerUp>>",
       a.rank_detail AS "rank_detail?: sqlx::types::Json<serde_json::Value>",
       a.injection_count, a.created_at AS "created_at!"
  FROM memory.consolidation_actions a
  JOIN memory.consolidation_runs r ON r.id = a.run_id
  JOIN memory.consolidation_run_kinds rk ON rk.id = r.run_kind_id
  LEFT JOIN projects.projects p ON p.id = a.project_id
  JOIN memory.consolidation_action_kinds ak ON ak.id = a.action_kind_id
  JOIN memory.consolidation_outcomes oc ON oc.id = a.outcome_id
  LEFT JOIN memory.lesson_stages bs ON bs.id = a.before_stage_id
  LEFT JOIN memory.lesson_stages as_ ON as_.id = a.after_stage_id
  LEFT JOIN memory.consolidation_decided_by db ON db.id = a.decided_by_id
 WHERE a.run_id = $1
 ORDER BY a.id ASC
