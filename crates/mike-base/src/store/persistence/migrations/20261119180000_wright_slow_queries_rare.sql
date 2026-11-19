-- slow_queries kept a statement's last window verdict until its window reached min_calls calls,
-- and a window only moves on once it has them. A statement that was slow once and is now called
-- a few times a day kept its old mean for good: on 2026-11-19 every one of 290 runs failed, with
-- open problems such as the old knowledge search (2 calls in the window, carrying 1593 ms) and
-- the eval class map (4 calls, carrying 281 ms). The last verdict now stands only while the
-- window is younger than quiet_hours (24), the time just after the base moved on. A window older
-- than that with fewer than min_calls calls is not judged, as a statement with fewer than
-- min_calls calls in all is not, so its problem heals; once it gathers min_calls calls it is
-- judged on their mean.
UPDATE wright.checks
   SET query = replace(
                   replace(query,
                           $old$                ELSE COALESCE((w.pe->>'window_mean_ms')::numeric, w.total_ms / w.calls)$old$,
                           $new$                WHEN w.base_at < now() - make_interval(hours => ($1->>'quiet_hours')::int) THEN NULL
                ELSE COALESCE((w.pe->>'window_mean_ms')::numeric, w.total_ms / w.calls)$new$),
                   $old$                ELSE COALESCE((w.pe->>'window_blocks_per_call')::numeric, w.blocks / w.calls)$old$,
                   $new$                WHEN w.base_at < now() - make_interval(hours => ($1->>'quiet_hours')::int) THEN NULL
                ELSE COALESCE((w.pe->>'window_blocks_per_call')::numeric, w.blocks / w.calls)$new$),
       expects = expects || '; a window older than quiet_hours with fewer than min_calls calls is not judged',
       updated_at = now()
 WHERE name = 'slow_queries'
   AND query LIKE '%ELSE COALESCE((w.pe->>''window_mean_ms'')%'
   AND query NOT LIKE '%THEN NULL%ELSE COALESCE((w.pe->>''window_mean_ms'')%';
