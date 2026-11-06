SELECT r.id AS "id!", r.created_at AS "created_at!", s.ordinal AS "ordinal?",
       CASE WHEN octet_length(s.entry->>'stage') <= 128 THEN s.entry->>'stage' END AS "stage?",
       CASE WHEN s.entry->>'outcome' IN ('applied','unchanged','skipped','failed_closed') THEN s.entry->>'outcome' END AS "outcome?",
       COALESCE(r.transformation_manifest->'schema_version' = '1'::jsonb
         AND CASE WHEN jsonb_typeof(r.transformation_manifest->'stages') = 'array'
                  THEN jsonb_array_length(r.transformation_manifest->'stages') <= 64 ELSE false END
         AND (s.ordinal IS NULL OR s.entry->'ordinal' = to_jsonb(s.ordinal)), false) AS "valid!",
       CASE WHEN s.entry->>'stage' = 'filter'
             AND jsonb_typeof(s.entry->'resolved_parameters'->'firing_rule_ids') = 'array'
            THEN jsonb_array_length(s.entry->'resolved_parameters'->'firing_rule_ids')
       END AS "firing_rule_count?"
  FROM requests.requests r
  LEFT JOIN LATERAL jsonb_array_elements(CASE
      WHEN r.transformation_manifest->'schema_version' = '1'::jsonb
       AND CASE WHEN jsonb_typeof(r.transformation_manifest->'stages') = 'array'
                    THEN jsonb_array_length(r.transformation_manifest->'stages') <= 64 ELSE false END
      THEN r.transformation_manifest->'stages' ELSE '[]'::jsonb END)
      WITH ORDINALITY AS s(entry, ordinal) ON true
 WHERE r.id = ANY($1) ORDER BY r.id, s.ordinal
