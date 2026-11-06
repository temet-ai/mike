SELECT r.id, r.finished_at FROM eval.rollups r
  WHERE r.finished_at IS NOT NULL AND r.finished_at <= '2026-10-14 04:00:00+00'::timestamptz
    AND EXISTS (SELECT 1 FROM eval.rates x
                 WHERE x.rollup_id = r.id AND x.family = 'steer_calibration')
  ORDER BY r.finished_at DESC, r.id DESC LIMIT 1;
