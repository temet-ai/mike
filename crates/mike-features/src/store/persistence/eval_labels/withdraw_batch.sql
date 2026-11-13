INSERT INTO eval.labels
        (subject_kind, subject_id, operator_turn_id, session_id, session_uuid, axis, label, action,
         tier, method, split, tags, severity, confidence, evidence, source, provenance,
         inclusion_prob, import_batch_id, supersedes_id)
 SELECT l.subject_kind, l.subject_id, l.operator_turn_id, l.session_id, l.session_uuid, l.axis,
        l.label, 'withdraw', l.tier, l.method, l.split, l.tags, l.severity, l.confidence, l.evidence,
        l.source, jsonb_build_object('withdrawn_reason', $2::text, 'withdrawn_by', $3::text),
        l.inclusion_prob, l.import_batch_id, l.id
   FROM eval.labels_as_of(now()) l
   JOIN eval.import_batches b ON b.id = l.import_batch_id
  WHERE b.source_key = $1
