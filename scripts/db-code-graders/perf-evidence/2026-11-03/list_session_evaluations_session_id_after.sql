WITH filtered AS (
    SELECT e.id, e.uuid, e.steerer_id, st.name AS steerer_name, st.kind AS steerer_kind,
           e.operator_turn_id, e.status, e.mode, e.question, e.trigger_point,
           e.trigger_request_id, e.bundle_id, e.intervention_uuid, e.dispatched_at,
           e.resolved_at, e.verdict_json, e.confidence, e.reason, e.outcome_label,
           e.label_source, e.labelled_at, e.label_evidence,
           count(*) OVER () AS total_count
      FROM steering.evaluations e
      JOIN steering.steerers st ON st.id = e.steerer_id
     WHERE e.session_id = 1720180 AND e.mode = 'shadow')
SELECT f.id, f.uuid, f.steerer_id, f.steerer_name, f.steerer_kind, f.operator_turn_id, f.status, f.mode, f.question, f.trigger_point, f.trigger_request_id, f.bundle_id, f.intervention_uuid, f.dispatched_at, f.resolved_at, f.verdict_json, f.confidence, f.reason, f.outcome_label, f.label_source, f.labelled_at, f.label_evidence, f.total_count FROM filtered f WHERE f.id < 9223372036854775807 ORDER BY f.id DESC LIMIT 50
