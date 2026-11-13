SELECT i.item_kind, i.route,
       count(*) AS "supplied!",
       coalesce(sum(octet_length(i.item_text)), 0)::bigint AS "bytes!",
       count(v.supplied_item_id) AS "judged!",
       count(*) FILTER (WHERE v.verdict = 'used') AS "used!",
       count(*) FILTER (WHERE v.verdict = 'corrected_anyway') AS "corrected_anyway!",
       count(*) FILTER (WHERE v.verdict = 'ignored') AS "ignored!",
       coalesce(sum(octet_length(i.item_text)) FILTER (WHERE v.verdict = 'ignored'), 0)::bigint
           AS "bytes_ignored!"
  FROM memory.supplied_items i
  LEFT JOIN memory.supplied_item_verdicts v ON v.supplied_item_id = i.id
 WHERE i.supplied_at >= $1
 GROUP BY i.item_kind, i.route
 ORDER BY i.item_kind, i.route
