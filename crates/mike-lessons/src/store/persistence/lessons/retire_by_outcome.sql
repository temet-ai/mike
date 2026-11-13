WITH judged AS (
    SELECT i.lesson_id, i.session_id,
           bool_or(COALESCE(v.hand_verdict, v.verdict) = 'used') AS used,
           bool_or(COALESCE(v.hand_verdict, v.verdict) = 'corrected_anyway') AS corrected
      FROM memory.supplied_items i
      JOIN memory.supplied_item_verdicts v ON v.supplied_item_id = i.id
      JOIN memory.lessons l ON l.id = i.lesson_id AND l.retired_at IS NULL AND l.archived_at IS NULL
     WHERE i.item_kind = 'lesson'
     GROUP BY i.lesson_id, i.session_id
), counts AS (
    SELECT j.lesson_id, count(*) AS judged,
           count(*) FILTER (WHERE j.used) AS used,
           count(*) FILTER (WHERE NOT j.used AND j.corrected) AS corrected,
           count(*) FILTER (WHERE NOT j.used AND NOT j.corrected) AS ignored
      FROM judged j
     GROUP BY j.lesson_id
), due AS (
    SELECT c.* FROM counts c
     WHERE c.judged >= $1::bigint AND c.used::float8 / c.judged < $2::float8
)
UPDATE memory.lessons l
   SET retired_at = now(), retired_kind = 'outcome',
       retired_reason = format(
           'Followed in %s of the %s judged sessions it was supplied to (%s%%), below the floor of %s%% over at least %s; corrected anyway in %s, ignored in %s.',
           d.used, d.judged, round(100.0 * d.used / d.judged), round(100 * $2::numeric), $1::bigint,
           d.corrected, d.ignored)
  FROM due d
 WHERE l.id = d.lesson_id AND l.retired_at IS NULL
RETURNING l.id
