WITH page AS (
    SELECT e.id, e.uuid, e.session_id, e.operator_turn_id, e.question, e.arm, e.dispatched_at,
           e.outcome_label, e.label_source, i.delivered_request_id, i.wake_state,
           i.delivery_channel, i.payload_text
      FROM steering.evaluations e
      JOIN steering.interventions i ON i.uuid = e.intervention_uuid
     WHERE e.steerer_id = 18
       AND (i.delivered_request_id IS NOT NULL OR i.pushed_at IS NOT NULL)
     ORDER BY e.dispatched_at DESC, e.id DESC
     LIMIT 50 OFFSET 0
)
SELECT page.uuid, s.session_uuid, s.title AS session_title, p.project_path,
       COALESCE(first_op.user_prompt, first_text.human_prompt) AS session_first_prompt,
       page.session_id, page.operator_turn_id, page.question, page.arm, page.dispatched_at,
       page.outcome_label, page.label_source, page.delivered_request_id, page.wake_state,
       page.delivery_channel, page.payload_text AS note_text,
       rx.reaction AS outcome_reaction
  FROM page
  JOIN sessions.sessions s ON s.id = page.session_id
  LEFT JOIN projects.projects p ON p.id = s.project_id
  LEFT JOIN LATERAL (
      SELECT v.reaction
        FROM memory.turn_verdicts v
       WHERE v.operator_turn_id = page.operator_turn_id AND v.mode = 'live'
       ORDER BY v.created_at DESC, v.id DESC
       LIMIT 1
  ) rx ON true
  LEFT JOIN LATERAL (
      SELECT COALESCE(ot.user_prompt, sr.human_prompt) AS user_prompt
        FROM (
            SELECT pk_cls.request_id
              FROM prompt_kinds.classifications pk_cls
             WHERE s.title IS NULL
               AND pk_cls.session_id = page.session_id
               AND pk_cls.kind_id = (SELECT id FROM prompt_kinds.kinds WHERE code = 'operator')
             ORDER BY pk_cls.request_id ASC
             LIMIT 1
        ) first_cls
        JOIN memory.operator_turns ot ON ot.start_request_id = first_cls.request_id
        LEFT JOIN requests.requests sr ON sr.id = first_cls.request_id
  ) first_op ON true
  LEFT JOIN LATERAL (
      SELECT r_any.human_prompt
        FROM requests.requests r_any
       WHERE s.title IS NULL
         AND first_op.user_prompt IS NULL
         AND r_any.session_id = page.session_id
         AND r_any.human_prompt IS NOT NULL
         AND r_any.human_prompt <> ''
         AND NOT EXISTS (
             SELECT 1 FROM prompt_kinds.classifications pk_any
              WHERE pk_any.request_id = r_any.id
         )
         AND r_any.turn_origin_id IN (
             SELECT id FROM requests.turn_origins
              WHERE code IN ('primary', 'primary_first', 'subagent', 'workflow_subagent')
         )
       ORDER BY r_any.created_at ASC, r_any.id ASC
       LIMIT 1
  ) first_text ON true
 ORDER BY page.dispatched_at DESC, page.id DESC
