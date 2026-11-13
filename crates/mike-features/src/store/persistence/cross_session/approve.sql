WITH proposal AS (
    UPDATE memory.directive_proposals
       SET status = 'approved', decided_at = now(), approved_text = $2
     WHERE id = $1 AND status = 'open'
    RETURNING id
),
provenance AS (
    SELECT operator_turn_id, session_id
      FROM memory.directive_proposal_evidence
     WHERE proposal_id = $1
     ORDER BY observed_at DESC
     LIMIT 1
),
targets AS (
    SELECT DISTINCT t.project_id FROM unnest($3::bigint[]) AS t(project_id)
),
inserted_lessons AS (
    INSERT INTO memory.lessons
        (project_id, kind_id, text, text_sha256, stage_id, source_id, source_turn_id, session_id, expires_at)
    SELECT tg.project_id, lk.id, $2, $4, ls.id, lsrc.id, pv.operator_turn_id, pv.session_id, $5
      FROM proposal p
      CROSS JOIN targets tg
      LEFT JOIN provenance pv ON true
      CROSS JOIN memory.lesson_kinds lk
      CROSS JOIN memory.lesson_stages ls
      CROSS JOIN memory.lesson_sources lsrc
     WHERE lk.code = $6 AND ls.code = 'daily' AND lsrc.code = 'directive'
    RETURNING id, project_id
)
INSERT INTO memory.directive_lessons (proposal_id, lesson_id, project_id)
SELECT $1, il.id, il.project_id
  FROM inserted_lessons il
RETURNING lesson_id AS "lesson_id!", project_id AS "project_id!"
