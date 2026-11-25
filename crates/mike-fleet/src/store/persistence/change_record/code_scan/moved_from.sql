SELECT e.before_sha AS "before_sha!"
  FROM workspace.change_events c
  JOIN workspace.change_events e
    ON e.id BETWEEN c.id - 2000 AND c.id + 2000
   AND e.at BETWEEN c.at - interval '1 minute' AND c.at + interval '1 minute'
 WHERE c.id = $1
   AND e.kind IN ('edit', 'shell_change')
   AND e.after_sha IS NULL
   AND e.before_sha IS NOT NULL
   AND cardinality(e.paths) = 1
   AND (e.before_sha = c.after_sha
        OR (e.checkout = c.checkout
            AND regexp_replace(e.paths[1], '^.*/', '') = regexp_replace(c.paths[1], '^.*/', '')))
 ORDER BY (e.before_sha = c.after_sha) DESC, abs(e.id - c.id)
 LIMIT 1
