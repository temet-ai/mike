WITH due AS (
     SELECT DISTINCT ON (r.to_lesson_id)
            r.id AS relation_id, r.from_lesson_id AS new_id, r.to_lesson_id AS old_id
       FROM memory.lesson_relations r
       JOIN memory.lesson_relation_statuses s ON s.id = r.status_id AND s.code = 'confirmed'
       JOIN memory.lesson_relation_kinds k ON k.id = r.kind_id
       JOIN memory.lesson_relation_methods m ON m.id = r.method_id
       JOIN memory.lessons nl ON nl.id = r.from_lesson_id AND nl.archived_at IS NULL
       JOIN memory.lessons ol ON ol.id = r.to_lesson_id AND ol.archived_at IS NULL
       JOIN memory.lesson_stages ns ON ns.id = nl.stage_id
       JOIN memory.lesson_sources nsrc ON nsrc.id = nl.source_id
       LEFT JOIN memory.lesson_shape_findings open_shape
         ON open_shape.lesson_id = nl.id AND open_shape.resolution IS NULL
      WHERE (k.code = 'supersedes' OR (k.code = 'refines' AND m.code = 'write_time'))
        AND (ns.code IN ('daily', 'long_term')
             OR (ns.code = 'raw' AND nsrc.code = 'operator' AND open_shape.lesson_id IS NULL))
      ORDER BY r.to_lesson_id, r.decided_at DESC NULLS LAST, r.id DESC
 ),
 archived AS (
     UPDATE memory.lessons l
        SET stage_id = a.id, archived_at = now(), archived_reason_id = ar.id
       FROM due d, memory.lesson_stages a, memory.lesson_archived_reasons ar
      WHERE l.id = d.old_id AND a.code = 'archived' AND ar.code = 'superseded'
     RETURNING l.id AS old_id
 ),
 moved AS (
     UPDATE memory.lesson_proposal_evidence e
        SET lesson_id = d.new_id
       FROM due d
       JOIN archived ar ON ar.old_id = d.old_id
      WHERE e.lesson_id = d.old_id
     RETURNING d.old_id AS old_id
 ),
 moved_counts AS (
     SELECT old_id, count(*)::int AS n FROM moved GROUP BY old_id
 ),
 injected AS (
     SELECT i.lesson_id AS old_id, count(*)::int AS n
       FROM memory.lesson_injections i
       JOIN archived ar ON ar.old_id = i.lesson_id
      GROUP BY i.lesson_id
 ),
 inherited AS (
     INSERT INTO memory.lesson_relations
            (from_lesson_id, to_lesson_id, kind_id, method_id, status_id, candidate_distance,
             evidence_json, proposer_model, proposer_rationale, adjudicator_model,
             adjudicator_agrees, adjudicator_rationale, detector_version, decided_at, decided_by)
     SELECT CASE WHEN r.from_lesson_id = d.old_id THEN d.new_id ELSE r.from_lesson_id END,
            CASE WHEN r.to_lesson_id = d.old_id THEN d.new_id ELSE r.to_lesson_id END,
            r.kind_id, r.method_id, r.status_id, r.candidate_distance,
            r.evidence_json || jsonb_build_object('inherited_from_lesson', d.old_id),
            r.proposer_model, r.proposer_rationale, r.adjudicator_model,
            r.adjudicator_agrees, r.adjudicator_rationale, r.detector_version,
            r.decided_at, r.decided_by
       FROM due d
       JOIN archived ar ON ar.old_id = d.old_id
       JOIN memory.lesson_relations r
         ON (r.from_lesson_id = d.old_id OR r.to_lesson_id = d.old_id)
        AND r.id <> d.relation_id
      WHERE CASE WHEN r.from_lesson_id = d.old_id THEN d.new_id ELSE r.from_lesson_id END
         <> CASE WHEN r.to_lesson_id = d.old_id THEN d.new_id ELSE r.to_lesson_id END
     ON CONFLICT (from_lesson_id, to_lesson_id, kind_id) DO NOTHING
     RETURNING id
 ),
 stamped AS (
     UPDATE memory.lesson_relations r
        SET evidence_json = r.evidence_json || jsonb_build_object(
                'applied_at', now(),
                'archived_lesson_id', d.old_id,
                'inherited_injections', COALESCE(inj.n, 0),
                'moved_evidence', COALESCE(mv.n, 0))
       FROM due d
       JOIN archived ar ON ar.old_id = d.old_id
       LEFT JOIN injected inj ON inj.old_id = d.old_id
       LEFT JOIN moved_counts mv ON mv.old_id = d.old_id
      WHERE r.id = d.relation_id
     RETURNING r.id
 )
 SELECT count(*) FROM archived
