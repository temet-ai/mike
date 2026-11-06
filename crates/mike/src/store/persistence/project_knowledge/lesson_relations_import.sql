INSERT INTO memory.lesson_relations(from_lesson_id, to_lesson_id, kind_id, method_id,
    status_id, evidence_json, proposer_model, proposer_rationale, adjudicator_model,
    adjudicator_agrees, adjudicator_rationale, detector_version, decided_by)
SELECT $1, target.id, k.id, m.id, st.id, COALESCE(x.evidence, '{}'),
    x.proposer_model, x.proposer_rationale, x.adjudicator_model, x.adjudicator_agrees,
    x.adjudicator_rationale, x.detector_version, x.decided_by
FROM jsonb_to_recordset($2) AS x(target_uuid uuid, kind text, method text, status text,
    evidence jsonb, proposer_model text, proposer_rationale text, adjudicator_model text,
    adjudicator_agrees boolean, adjudicator_rationale text, detector_version text, decided_by text)
JOIN memory.lessons target ON target.uuid = x.target_uuid AND target.project_id = $3
JOIN memory.lesson_relation_kinds k ON k.code = x.kind
JOIN memory.lesson_relation_methods m ON m.code = x.method
JOIN memory.lesson_relation_statuses st ON st.code = x.status
ON CONFLICT(from_lesson_id, to_lesson_id, kind_id) DO UPDATE
    SET status_id = EXCLUDED.status_id, evidence_json = EXCLUDED.evidence_json,
        method_id = EXCLUDED.method_id, decided_by = EXCLUDED.decided_by,
        proposer_model = EXCLUDED.proposer_model, proposer_rationale = EXCLUDED.proposer_rationale,
        adjudicator_model = EXCLUDED.adjudicator_model, adjudicator_agrees = EXCLUDED.adjudicator_agrees,
        adjudicator_rationale = EXCLUDED.adjudicator_rationale, detector_version = EXCLUDED.detector_version
