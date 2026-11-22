WITH ranked AS (
    SELECT c.queryid, c.taken_at, c.calls, c.stats_since,
           row_number() OVER w AS n,
           first_value(c.calls) OVER w AS newest_calls,
           first_value(c.stats_since) OVER w AS newest_since,
           first_value(c.taken_at) OVER w AS newest_at
      FROM wright.statement_counters c
    WINDOW w AS (PARTITION BY c.queryid ORDER BY c.taken_at DESC)
)
DELETE FROM wright.statement_counters c
 USING ranked r
 WHERE r.queryid = c.queryid
   AND r.taken_at = c.taken_at
   AND (r.n > 2
        OR r.calls > r.newest_calls
        OR r.stats_since IS DISTINCT FROM r.newest_since
        OR r.newest_at < now() - make_interval(hours => $1))
