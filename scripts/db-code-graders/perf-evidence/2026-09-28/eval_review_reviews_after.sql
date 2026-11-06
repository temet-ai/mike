SELECT l.id, l.subject_kind, l.subject_id, l.operator_turn_id, l.session_id, l.session_uuid, l.axis, l.label, l.action, l.tier, l.method, l.split, l.tags, l.severity, l.confidence, l.evidence, l.note, l.source, l.provenance, l.inclusion_prob, l.verdict_snapshot, l.verdict_bundle_id, l.import_batch_id, l.supersedes_id, l.created_at FROM eval.labels l
          WHERE l.subject_kind = 'turn' AND l.subject_id = 13778 AND l.tier = 'gold'
            AND l.action = 'assert' AND l.provenance ->> 'eval_run_id' = '3'
          ORDER BY l.id DESC
