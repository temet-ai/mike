WITH filtered AS (
    SELECT e.id, e.uuid, e.steerer_id, st.name AS steerer_name, st.kind AS steerer_kind,
           e.operator_turn_id, e.status, e.mode, e.question, e.trigger_point,
           e.trigger_request_id, e.bundle_id, e.intervention_uuid, e.dispatched_at,
           e.resolved_at, e.verdict_json, e.confidence, e.reason, e.outcome_label,
           e.label_source, e.labelled_at, e.label_evidence,
           count(*) OVER () AS total_count
      FROM steering.evaluations e
      JOIN sessions.sessions s ON s.id = e.session_id
      JOIN steering.steerers st ON st.id = e.steerer_id
     WHERE s.session_uuid = 'b9fec1e5-064d-40b0-8c97-fb6822d41863'::uuid
       AND (NULL::bigint IS NULL OR e.steerer_id = NULL)
       AND ('shadow'::text IS NULL OR e.mode = 'shadow'))
SELECT f.* FROM filtered f WHERE (NULL::bigint IS NULL OR f.id < NULL) ORDER BY f.id DESC LIMIT 50
