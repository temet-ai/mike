SELECT id, finished_at FROM eval.rollups
  WHERE finished_at IS NOT NULL AND finished_at <= '2026-10-14 04:00:00+00'::timestamptz
  ORDER BY finished_at DESC, id DESC LIMIT 1;
