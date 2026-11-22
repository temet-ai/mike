-- slow_queries kept a statement flagged for quiet_hours (24 h) after its last
-- call on the old window mean, and reopened a closed problem from evidence
-- that had not changed since it closed: problem 61944 (queryid
-- -6430946467875054203, unchanged at 158 calls since 2026-11-22 14:12 UTC,
-- its code replaced in e9d9c8c47) reopened at 17:21 UTC right after
-- migration 20261120166800 closed it.
--
-- slow_queries now leaves a statement unflagged, so its problem heals, when
--   * no call reached it in a window older than stale_grace_minutes
--     (window_calls = 0), or
--   * its calls are unchanged since the last episode's evidence and that
--     episode is closed (no reopen from unchanged evidence), or open but
--     opened more than stale_grace_minutes after the last call (the episode
--     was opened from a statement that had already stopped).
-- A slow statement still being called keeps its quiet_hours continuity.
--
-- idle_polls judged a window younger than min_window_minutes on the previous
-- window's rate, so a poll that had stopped stayed flagged, and a closed
-- problem could reopen from that carried rate. It now needs at least one
-- call since its last evidence (a poll at 60/h or more always has one
-- between two passes), and a ready window with no call heals.

UPDATE wright.checks
   SET query = replace(replace(replace(replace(query,
           $a$    SELECT p.resource, e.evidence AS pe, e.closed_at IS NULL AS open$a$,
           $b$    SELECT p.resource, e.evidence AS pe, e.closed_at IS NULL AS open, e.opened_at AS episode_opened_at$b$),
           $a$         SELECT ep.evidence, ep.closed_at
$a$,
           $b$         SELECT ep.evidence, ep.closed_at, ep.opened_at
$b$),
           $a$    SELECT n.*, pr.pe, COALESCE(pr.open, false) AS open,$a$,
           $b$    SELECT n.*, pr.pe, COALESCE(pr.open, false) AS open, pr.episode_opened_at,$b$),
           $a$       AND w.last_called_at >= now() - make_interval(hours => ($1->>'quiet_hours')::int)
$a$,
           $b$       AND w.last_called_at >= now() - make_interval(hours => ($1->>'quiet_hours')::int)
       AND NOT (w.window_calls = 0
                AND w.base_at < now() - make_interval(mins => ($1->>'stale_grace_minutes')::int))
       AND NOT (w.pe IS NOT NULL AND NOT w.fresh
                AND w.calls = (w.pe->>'calls')::numeric
                AND (NOT w.open
                     OR w.last_called_at < w.episode_opened_at
                                           - make_interval(mins => ($1->>'stale_grace_minutes')::int)))
$b$),
       params = params || '{"stale_grace_minutes": 60}'::jsonb,
       expects = expects || '; a statement with no call since its window began, or unchanged since a closed episode or since before its open episode began, is not reported',
       updated_at = now()
 WHERE name = 'slow_queries'
   AND query NOT LIKE '%stale_grace_minutes%';

UPDATE wright.checks
   SET query = replace(query,
           $a$ WHERE j.calls_per_hour >= ($1->>'calls_per_hour')::numeric
$a$,
           $b$ WHERE j.calls_per_hour >= ($1->>'calls_per_hour')::numeric
   AND (j.pe IS NULL OR j.fresh OR j.calls > (j.pe->>'calls')::numeric)
   AND NOT (j.ready AND j.window_calls = 0)
$b$),
       expects = expects || '; a statement with no call since its last evidence is not reported',
       updated_at = now()
 WHERE name = 'idle_polls'
   AND query NOT LIKE '%j.calls > (j.pe->>''calls'')%';
