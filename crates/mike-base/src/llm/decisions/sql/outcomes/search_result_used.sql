WITH calls AS (
    SELECT r.id AS record_id, r.created_at, r.state ->> 'query' AS query,
           (r.composed -> 'verdict' ->> 'search_event_id')::bigint AS event_id,
           r.composed -> 'answers' AS answers
      FROM decisions.records r
     WHERE r.point = 'search_grade'
       AND r.applied = 'labelled'
       AND r.created_at >= now() - interval '30 days'
),
exact AS (
    SELECT c.record_id, c.answers, e.id AS event_id, e.returned, e.used_references
      FROM calls c
      JOIN memory.search_events e ON e.id = c.event_id
     WHERE e.used_final_at IS NOT NULL
),
by_query AS (
    SELECT DISTINCT ON (c.record_id)
           c.record_id, c.answers, e.id AS event_id, e.returned, e.used_references
      FROM calls c
      JOIN memory.search_events e
        ON e.query = c.query
       AND e.tool = 'search_project_knowledge'
       AND e.created_at >= c.created_at - interval '7 days'
       AND e.created_at <= c.created_at
     WHERE c.event_id IS NULL
       AND e.used_final_at IS NOT NULL
     ORDER BY c.record_id, e.created_at DESC, e.id DESC
),
events AS (
    SELECT * FROM exact
    UNION ALL
    SELECT * FROM by_query
),
graded AS (
    SELECT ev.record_id, ev.event_id,
           (a.value ->> 'noul')::float8 AS noul,
           (ev.returned -> (substring(a.key FROM 8)::int - 1)) ->> 'reference' AS reference,
           ev.used_references
      FROM events ev
      CROSS JOIN LATERAL jsonb_each(ev.answers) a(key, value)
     WHERE a.key ~ '^result_[0-9]+$'
       AND a.value ->> 'noul' IS NOT NULL
),
per_record AS (
    SELECT g.record_id, min(g.event_id) AS event_id, count(*) AS graded,
           bool_or(g.noul >= 0.5 AND g.reference = ANY (g.used_references)) AS relevant_cited,
           bool_or(g.noul < 0.5 AND g.reference = ANY (g.used_references)) AS irrelevant_cited,
           bool_and(g.noul < 0.5) AS all_irrelevant,
           bool_or(g.reference = ANY (g.used_references)) AS any_cited
      FROM graded g
     WHERE g.reference IS NOT NULL
     GROUP BY g.record_id
),
labelled AS (
    SELECT p.*,
           CASE
               WHEN p.relevant_cited THEN 'right'
               WHEN p.irrelevant_cited THEN 'wrong'
               WHEN p.all_irrelevant AND NOT p.any_cited THEN 'right'
           END AS label
      FROM per_record p
)
INSERT INTO decisions.outcomes (record_id, label, label_source, evidence)
SELECT l.record_id, l.label, 'search_result_used',
       jsonb_build_object('search_event_id', l.event_id, 'graded_results', l.graded,
                          'relevant_cited', l.relevant_cited,
                          'irrelevant_cited', l.irrelevant_cited)
  FROM labelled l
 WHERE l.label IS NOT NULL
   AND NOT EXISTS (SELECT 1 FROM decisions.outcomes o
                    WHERE o.record_id = l.record_id AND o.label_source = 'search_result_used')
ON CONFLICT (record_id, label_source, label) DO NOTHING
