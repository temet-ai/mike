UPDATE memory.lesson_shape_findings
    SET resolution = 'kept',
        resolved_at = now(),
        last_error = COALESCE(last_error, 'released: no rewrite within ' || 14::integer::text || ' days')
  WHERE resolution IS NULL
    AND found_at < now() - make_interval(days => 14)
