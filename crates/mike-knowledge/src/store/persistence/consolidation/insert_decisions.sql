INSERT INTO memory.knowledge_consolidation_decisions
    (run_id, project_id, subject_id, statement_id, item_id, carrier_reference, verdict,
     counterpart_statement_id, counterpart_reference, text_before, text_after, reason, evidence, model,
     application, applied_at, error)
SELECT d.run_id, d.project_id, d.subject_id, d.statement_id, d.item_id, d.carrier_reference, d.verdict,
       d.counterpart_statement_id, d.counterpart_reference, d.text_before, d.text_after, d.reason,
       COALESCE(d.evidence, '[]'::jsonb), d.model, d.application, d.applied_at, d.error
  FROM jsonb_to_recordset($1::jsonb) AS d(
       run_id bigint, project_id bigint, subject_id bigint, statement_id bigint, item_id bigint,
       carrier_reference text, verdict text, counterpart_statement_id bigint, counterpart_reference text,
       text_before text, text_after text, reason text, evidence jsonb, model text,
       application text, applied_at timestamptz, error text)
RETURNING id
