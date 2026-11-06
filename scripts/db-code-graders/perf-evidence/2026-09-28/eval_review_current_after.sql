SELECT DISTINCT ON (l.axis) l.id, l.subject_kind, l.subject_id, l.operator_turn_id, l.session_id, l.session_uuid, l.axis, l.label, l.action, l.tier, l.method, l.split, l.tags, l.severity, l.confidence, l.evidence, l.note, l.source, l.provenance, l.inclusion_prob, l.verdict_snapshot, l.verdict_bundle_id, l.import_batch_id, l.supersedes_id, l.created_at FROM eval.labels_current_v l
          WHERE l.subject_kind = 'turn' AND l.subject_id = 13778
            AND l.axis IN ('reaction', 'delivery') AND l.tier IN ('gold', 'relayed', 'seed')
          ORDER BY l.axis DESC,
                   CASE l.tier WHEN 'gold' THEN 1 WHEN 'relayed' THEN 2 ELSE 3 END,
                   l.created_at DESC, l.id DESC
