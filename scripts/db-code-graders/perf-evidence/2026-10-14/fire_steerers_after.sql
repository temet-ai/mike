SELECT id, name,
        coalesce((config ->> 'confidence_threshold')::float8, 0.7)::float4 AS static_threshold
   FROM steering.steerers
  WHERE name = ANY(ARRAY['verifier', 'monitor-over-sleep']) AND deleted_at IS NULL
  ORDER BY id;
