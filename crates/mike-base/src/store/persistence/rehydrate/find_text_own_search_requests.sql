SELECT DISTINCT tu.request_id AS "request_id!"
FROM tools.uses tu
WHERE tu.request_id = ANY($1)
  AND EXISTS (
        SELECT 1
        FROM unnest($2::text[]) AS suffix(name)
        WHERE right(tu.tool_name, length(suffix.name)) = suffix.name
      )
  AND (CASE WHEN $4 THEN strpos(tu.input::text, $3)
            ELSE strpos(lower(tu.input::text), lower($3)) END) > 0
