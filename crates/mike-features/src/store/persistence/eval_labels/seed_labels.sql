INSERT INTO eval.labels
        (subject_kind, subject_id, operator_turn_id, session_id, session_uuid, axis, label, tier,
         method, split, tags, severity, confidence, evidence, source, provenance, inclusion_prob,
         import_batch_id)
 SELECT x.subject_kind, x.subject_id, x.subject_id, x.session_id, x.session_uuid::uuid, x.axis,
        x.label, 'seed', x.method, 'dev', x.tags, x.severity, x.confidence, x.evidence, x.source,
        x.provenance, x.inclusion_prob, $2
   FROM jsonb_to_recordset($1::jsonb) AS x(
        subject_kind text, subject_id bigint, session_id bigint, session_uuid text, axis text,
        label text, method text, tags text[], severity text, confidence text, evidence text,
        source text, provenance jsonb, inclusion_prob double precision)
