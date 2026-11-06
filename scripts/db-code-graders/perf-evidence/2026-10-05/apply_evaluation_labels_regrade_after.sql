UPDATE steering.evaluations e
    SET outcome_label = x.label,
        label_source = x.source,
        labelled_at = NOW(),
        label_evidence = x.evidence
   FROM jsonb_to_recordset('[{"uuid":"917f96e3-df3a-4709-a4c7-5dd07f0c7ade","label":"wrong","source":"session_reply","evidence":{"labeller_version":"reply-labeller-v2","reaction":"declined"}}]'::jsonb) AS x(uuid uuid, label text, source text, evidence jsonb)
  WHERE e.uuid = x.uuid
    AND e.status <> 'dispatched'
    AND (e.outcome_label IS NULL
         OR (x.source = 'operator' AND e.label_source IS DISTINCT FROM 'operator')
         OR (x.source = 'session_reply' AND e.label_source = 'session_reply'
             AND e.label_evidence->>'labeller_version'
                 IS DISTINCT FROM x.evidence->>'labeller_version'))
