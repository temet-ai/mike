WITH launches AS (
    SELECT l.id, l.seat, l.created_at, l.closed_at, l.opened_at, l.channel_checked_at,
           l.brief_mode = 'inline' AS by_hand,
           substring(l.seat from '-([A-Z]+[0-9]+)[a-z]*$') AS task,
           CASE WHEN l.seat LIKE 'coordinator%' OR l.seat LIKE 'observer%' OR l.seat LIKE 'acceptance%'
                     OR l.seat LIKE 'template-%' OR l.seat = 'reference' THEN false
                ELSE true END AS is_worker
      FROM launch.launches l
     WHERE l.run_id = 'fleet-radar-retire-ratatui-db99de21' AND l.seat IS NOT NULL AND l.seat <> 'status'
),
first_per_task AS (
    SELECT DISTINCT ON (task) task, created_at, by_hand
      FROM launches
     WHERE is_worker AND task IS NOT NULL
     ORDER BY task, created_at, id
),
last_auto AS (
    SELECT max(created_at) AS at FROM first_per_task WHERE NOT by_hand
),
hand_firsts AS (
    SELECT f.task, f.created_at,
           f.created_at - lag(f.created_at) OVER (ORDER BY f.created_at, f.task) AS gap
      FROM first_per_task f, last_auto a
     WHERE f.by_hand AND f.created_at > a.at
),
bursts AS (
    SELECT task, created_at,
           sum(CASE WHEN gap IS NULL OR gap > interval '60 seconds' THEN 1 ELSE 0 END)
               OVER (ORDER BY created_at, task) AS burst
      FROM hand_firsts
),
stalled AS (
    SELECT b.task, extract(epoch FROM b.created_at - a.at) / 60 AS minutes
      FROM bursts b, last_auto a
     WHERE b.burst = 1
       AND (SELECT min(created_at) FROM bursts) - a.at > interval '5 minutes'
),
bounds AS (
    SELECT min(created_at) AS first_launch, max(closed_at) AS last_closed FROM launches
)
SELECT coalesce(round((extract(epoch FROM b.last_closed - b.first_launch) / 60)::numeric, 1), 0)::float8 AS time_to_done_minutes,
       coalesce(round((greatest(extract(epoch FROM r.closed_at - b.last_closed), 0) / 60)::numeric, 1), 0)::float8 AS stop_minutes,
       coalesce((SELECT round((sum(greatest(extract(epoch FROM coalesce(channel_checked_at, opened_at) - created_at) - 30, 0)) / 60)::numeric, 1)
                   FROM launches), 0)::float8 AS relaunch_gap_minutes,
       coalesce((SELECT round(sum(minutes)::numeric, 1) FROM stalled), 0)::float8 AS no_seat_minutes,
       (SELECT count(*) FROM launches) AS seat_launches,
       (SELECT count(*) FROM launches WHERE by_hand) AS hand_starts,
       (SELECT count(DISTINCT task) FROM launches WHERE is_worker AND task IS NOT NULL) AS task_count,
       (SELECT count(*) FROM launches WHERE is_worker) AS worker_launches
  FROM bounds b
  LEFT JOIN launch.runs r ON r.run_id = 'fleet-radar-retire-ratatui-db99de21'
