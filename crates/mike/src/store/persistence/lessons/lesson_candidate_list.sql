SELECT l.id AS "id!", l.project_id, p.project_path AS "project_path?", lk.code AS "kind!", l.text AS "text!",
       lsrc.code AS "source!", ls.code AS "stage!", l.source_turn_id, l.created_at AS "created_at!",
       COALESCE(ec.evidence_count, 0) AS "evidence_count!",
       s.session_uuid AS "session_uuid?", s.title AS "session_title?", ot.closed_at AS "turn_closed_at?",
       ot.user_prompt AS "user_prompt?", ot.final_assistant_text AS "final_assistant_text?",
       r.human_prompt AS "correcting_message?",
       cl.runs_given AS "carry_runs_given?", cl.later_findings AS "carry_later_findings?"
  FROM memory.lessons l
  LEFT JOIN projects.projects p ON p.id = l.project_id
  JOIN memory.lesson_kinds lk ON lk.id = l.kind_id
  JOIN memory.lesson_stages ls ON ls.id = l.stage_id
  JOIN memory.lesson_sources lsrc ON lsrc.id = l.source_id
  LEFT JOIN (
      SELECT lesson_id, count(*) AS evidence_count
        FROM memory.lesson_proposal_evidence
       WHERE lesson_id IS NOT NULL
       GROUP BY lesson_id
  ) ec ON ec.lesson_id = l.id
  LEFT JOIN memory.carried_lessons cl ON cl.lesson_id = l.id
  LEFT JOIN memory.operator_turns ot ON ot.id = l.source_turn_id
  LEFT JOIN sessions.sessions s ON s.id = ot.session_id
  LEFT JOIN memory.turn_facts tf ON tf.operator_turn_id = ot.id
  LEFT JOIN requests.requests r ON r.id = tf.next_genuine_request_id
 WHERE ls.code = 'raw' AND lsrc.code IN ('judge', 'directive', 'carry')
 ORDER BY l.created_at DESC
 LIMIT $1
