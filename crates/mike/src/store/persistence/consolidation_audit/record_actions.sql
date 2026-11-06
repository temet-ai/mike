WITH candidates AS (
    SELECT u.lesson_id, u.lesson_text, u.action_kind, u.outcome, u.error,
           u.before_stage, u.after_stage, u.decided_by, u.distance, u.threshold,
           u.embedding_model, u.counterpart_lesson_id, u.counterpart_uuid, u.counterpart_text,
           u.runners_up, u.rank_detail
      FROM UNNEST(
               $2::bigint[], $3::text[], $4::text[], $5::text[], $6::text[],
               $7::text[], $8::text[], $9::text[], $10::real[], $11::real[],
               $12::text[], $13::bigint[], $14::uuid[], $15::text[], $16::jsonb[], $17::jsonb[]
           ) AS u(lesson_id, lesson_text, action_kind, outcome, error,
                  before_stage, after_stage, decided_by, distance, threshold,
                  embedding_model, counterpart_lesson_id, counterpart_uuid, counterpart_text,
                  runners_up, rank_detail)
),
-- Top-N-per-group (N=1) over EXISTING rows, not a
-- correlated subquery: DISTINCT ON resolves the whole set
-- in one pass, then the NOT EXISTS below probes it by
-- equality per candidate rather than re-aggregating per row.
latest AS (
    SELECT DISTINCT ON (a.lesson_id)
           a.lesson_id, ak2.code AS action_kind, oc2.code AS outcome, a.error
      FROM memory.consolidation_actions a
      JOIN memory.consolidation_action_kinds ak2 ON ak2.id = a.action_kind_id
      JOIN memory.consolidation_outcomes oc2 ON oc2.id = a.outcome_id
     WHERE a.lesson_id = ANY($2::bigint[])
     ORDER BY a.lesson_id, a.id DESC
)
INSERT INTO memory.consolidation_actions (
    run_id, lesson_id, lesson_uuid, project_id, agent_id, session_id, session_uuid,
    source_turn_id, files_touched, lesson_text,
    action_kind_id, outcome_id, error,
    before_stage_id, after_stage_id, decided_by_id,
    distance, threshold, embedding_model,
    counterpart_lesson_id, counterpart_uuid, counterpart_text,
    runners_up, rank_detail, injection_count
)
SELECT $1, l.id, l.uuid, l.project_id, l.agent_id, l.session_id, sess.session_uuid,
       l.source_turn_id, ot.files_touched, c.lesson_text,
       ak.id, oc.id, c.error,
       bs.id, as_.id, db.id,
       c.distance, c.threshold, c.embedding_model,
       c.counterpart_lesson_id, c.counterpart_uuid, c.counterpart_text,
       c.runners_up, c.rank_detail, COALESCE(inj.injection_count, 0)
  FROM candidates c
  JOIN memory.lessons l ON l.id = c.lesson_id
  LEFT JOIN sessions.sessions sess ON sess.id = l.session_id
  LEFT JOIN memory.operator_turns ot ON ot.id = l.source_turn_id
  JOIN memory.consolidation_action_kinds ak ON ak.code = c.action_kind
  JOIN memory.consolidation_outcomes oc ON oc.code = c.outcome
  LEFT JOIN memory.lesson_stages bs ON bs.code = c.before_stage
  LEFT JOIN memory.lesson_stages as_ ON as_.code = c.after_stage
  LEFT JOIN memory.consolidation_decided_by db ON db.code = c.decided_by
  LEFT JOIN (
      SELECT lesson_id, count(*)::integer AS injection_count
        FROM memory.lesson_injections
       GROUP BY lesson_id
  ) inj ON inj.lesson_id = c.lesson_id
 WHERE c.action_kind <> 'evaluated' OR c.outcome <> 'skipped'
    OR NOT EXISTS (
        SELECT 1 FROM latest lt
         WHERE lt.lesson_id = c.lesson_id
           AND lt.action_kind = c.action_kind
           AND lt.outcome = c.outcome
           AND lt.error IS NOT DISTINCT FROM c.error
    )
