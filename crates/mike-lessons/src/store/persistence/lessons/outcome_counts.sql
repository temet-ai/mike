WITH judged AS (
    SELECT i.lesson_id, i.session_id,
           bool_or(COALESCE(v.hand_verdict, v.verdict) = 'used') AS used,
           bool_or(COALESCE(v.hand_verdict, v.verdict) IN ('corrected_anyway', 'came_back')) AS corrected
      FROM memory.supplied_items i
      JOIN memory.supplied_item_verdicts v ON v.supplied_item_id = i.id
     WHERE i.item_kind = 'lesson'
       AND i.lesson_id = ANY($1::bigint[])
       AND COALESCE(v.hand_verdict, v.verdict) <> 'not_applicable'
     GROUP BY i.lesson_id, i.session_id
)
SELECT j.lesson_id,
       count(*) AS judged,
       count(*) FILTER (WHERE j.used) AS used,
       count(*) FILTER (WHERE NOT j.used AND j.corrected) AS corrected_anyway,
       count(*) FILTER (WHERE NOT j.used AND NOT j.corrected) AS ignored
  FROM judged j
 GROUP BY j.lesson_id
