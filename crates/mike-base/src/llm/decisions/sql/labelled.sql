SELECT DISTINCT ON (r.id)
       r.point AS "point!",
       (r.composed -> 'verdict' ->> 'score')::float8 AS "score?",
       r.applied AS "applied?",
       o.label AS "label!"
  FROM decisions.records r
  JOIN decisions.outcomes o ON o.record_id = r.id
  LEFT JOIN unnest($2::text[], $3::text[]) AS retired(point, label_source)
    ON retired.point = r.point AND retired.label_source = o.label_source
 WHERE r.created_at >= $1 AND r.mode IN ('on', 'shadow', 'replay')
   AND retired.point IS NULL
 ORDER BY r.id, o.labelled_at
