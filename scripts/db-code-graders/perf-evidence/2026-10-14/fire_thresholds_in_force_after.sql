SELECT DISTINCT ON (steerer_id)
        id, steerer_id, question, scope, project_id, computed_at, rollup_id, applied_threshold, decision, detail
   FROM steering.guard_thresholds
  WHERE question = 'fire' AND decision = 'applied' AND computed_at <= '2026-10-14 12:00:00+00'::timestamptz
  ORDER BY steerer_id, computed_at DESC, id DESC;
