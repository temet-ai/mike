WITH gold AS (
    SELECT DISTINCT ON (l.subject_id, l.axis)
           l.subject_id AS turn_id, l.axis, l.label
      FROM eval.labels_as_of('infinity'::timestamptz) l
     WHERE l.subject_kind = 'turn' AND l.tier = 'gold' AND l.source = 'spa'
       AND l.axis IN ('reaction', 'delivery')
     ORDER BY l.subject_id, l.axis, l.created_at DESC, l.id DESC
), axes AS (
    SELECT g.turn_id,
           max(g.label) FILTER (WHERE g.axis = 'reaction') AS reaction,
           max(g.label) FILTER (WHERE g.axis = 'delivery' AND g.label <> 'not_applicable') AS delivery
      FROM gold g
     GROUP BY g.turn_id
), judged AS (
    SELECT r.id AS record_id,
           a.turn_id,
           a.reaction AS gold_reaction,
           a.delivery AS gold_delivery,
           r.composed -> 'verdict' ->> 'reaction' AS jev_reaction,
           r.composed -> 'verdict' ->> 'delivery' AS jev_delivery
      FROM decisions.records r
      JOIN axes a ON a.turn_id = r.operator_turn_id
      LEFT JOIN decisions.outcomes done
        ON done.record_id = r.id AND done.label_source = 'operator_gold_axes'
     WHERE r.point = 'turn_judge'
       AND r.mode IN ('on', 'shadow')
       AND done.record_id IS NULL
       AND (a.reaction IS NOT NULL OR a.delivery IS NOT NULL)
       AND (a.reaction IS NULL OR r.composed -> 'verdict' ->> 'reaction' IS NOT NULL)
       AND (a.delivery IS NULL OR r.composed -> 'verdict' ->> 'delivery' IS NOT NULL)
)
INSERT INTO decisions.outcomes (record_id, label, label_source, evidence)
SELECT j.record_id,
       CASE WHEN (j.gold_reaction IS NULL OR j.gold_reaction = j.jev_reaction)
                 AND (j.gold_delivery IS NULL OR j.gold_delivery = j.jev_delivery)
            THEN 'right' ELSE 'wrong' END,
       'operator_gold_axes',
       jsonb_build_object('operator_turn_id', j.turn_id,
                          'gold_reaction', j.gold_reaction,
                          'jev_reaction', j.jev_reaction,
                          'gold_delivery', j.gold_delivery,
                          'jev_delivery', j.jev_delivery)
  FROM judged j
ON CONFLICT (record_id, label_source, label) DO NOTHING
