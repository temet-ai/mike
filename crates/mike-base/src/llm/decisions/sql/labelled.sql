SELECT DISTINCT ON (r.id)
       r.point AS "point!",
       (r.composed -> 'verdict' ->> 'score')::float8 AS "score?",
       r.applied AS "applied?",
       o.label AS "label!"
  FROM decisions.records r
  JOIN decisions.outcomes o ON o.record_id = r.id
 WHERE r.created_at >= $1 AND r.mode IN ('on', 'shadow', 'replay')
 ORDER BY r.id, o.labelled_at
