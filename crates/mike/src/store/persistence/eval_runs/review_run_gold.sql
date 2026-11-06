SELECT l.id,
       l.subject_kind,
       l.subject_id,
       l.operator_turn_id,
       l.session_id,
       l.session_uuid,
       l.axis,
       l.label,
       l.action,
       l.tier,
       l.method,
       l.split,
       l.tags,
       l.severity,
       l.confidence,
       l.evidence,
       l.note,
       l.source,
       l.provenance,
       l.inclusion_prob,
       l.verdict_snapshot,
       l.verdict_bundle_id,
       l.import_batch_id,
       l.supersedes_id,
       l.created_at
  FROM eval.labels l
 WHERE l.id IN (SELECT i.reaction_label_id FROM eval.run_items i
                 WHERE i.run_id = $1 AND i.operator_turn_id = $2
                UNION ALL
                SELECT i.delivery_label_id FROM eval.run_items i
                 WHERE i.run_id = $1 AND i.operator_turn_id = $2)
 ORDER BY l.axis DESC
