SELECT id, definition_sha256 AS "definition_sha256!"
   FROM tools.definitions
  WHERE definition_sha256 = ANY(ARRAY['064680a469507d32d83c736d5948e4a043ca09f2efa47f3f38404456137922c3']::text[])
