SELECT DISTINCT l.subject_id, t.closed_at
  FROM eval.labels_as_of('infinity'::timestamptz) l
  JOIN memory.operator_turns t ON t.id = l.subject_id
 WHERE l.subject_kind = 'turn' AND l.source = 'fleet_mirror/ground_truth'
   AND l.split = 'dev'
   AND t.closed_at >= '2026-07-27T00:00:00Z'
   AND t.closed_at < '2026-09-23T00:00:00Z'
   AND EXISTS (
       SELECT 1 FROM memory.semantic_items i
        WHERE i.operator_turn_id = l.subject_id
          AND i.embedder_key = 'nomic-embed-text-v1.5|768|1024|doc-prefix-v1|since:2026-09-25T10:23:37Z'
          AND i.kind IN ('turn_prompt', 'turn_ending'))
 ORDER BY l.subject_id
 LIMIT 300;
