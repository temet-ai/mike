-- slow_queries names a problem by queryid plus the statement's first 120 characters. Editing a
-- statement's SQL gives it a new queryid, so the old problem stayed open for quiet_hours (24) after
-- its last call while the same cost went on under the new queryid as a second problem (#391:
-- problem 2009 and six open "WITH own_turn AS" problems on 2026-11-18). A statement is now dropped
-- once it has had no call for superseded_hours (1) while a statement with the same leading text
-- and a different queryid is flagged and called within that hour, so its problem heals.
UPDATE wright.checks
   SET params = params || '{"superseded_hours": 1}'::jsonb,
       query = replace(
                   replace(query,
                           $old$)
SELECT 'query'::text AS scope,$old$,
                           $new$),
flagged AS (
    SELECT w.*, left(regexp_replace(btrim(w.query), '\s+', ' ', 'g'), 120) AS text_key
      FROM windowed w
     WHERE (w.window_mean_ms >= ($1->>'mean_ms')::numeric
            OR w.window_blocks_per_call >= ($1->>'blocks_per_call')::numeric)
       AND w.last_called_at >= now() - make_interval(hours => ($1->>'quiet_hours')::int)
)
SELECT 'query'::text AS scope,$new$),
                   $old$  FROM windowed j
 WHERE (j.window_mean_ms >= ($1->>'mean_ms')::numeric
        OR j.window_blocks_per_call >= ($1->>'blocks_per_call')::numeric)
   AND j.last_called_at >= now() - make_interval(hours => ($1->>'quiet_hours')::int)
 ORDER BY$old$,
                   $new$  FROM flagged j
 WHERE NOT (j.last_called_at < now() - make_interval(hours => COALESCE(($1->>'superseded_hours')::int, 1))
            AND EXISTS (SELECT 1
                          FROM flagged n
                         WHERE n.text_key = j.text_key
                           AND n.queryid <> j.queryid
                           AND n.last_called_at >= now() - make_interval(hours => COALESCE(($1->>'superseded_hours')::int, 1))))
 ORDER BY$new$),
       expects = expects || '; a statement with no call in superseded_hours is not reported while another queryid with the same leading text is flagged and still called',
       updated_at = now()
 WHERE name = 'slow_queries'
   AND query LIKE '%FROM windowed j%';
