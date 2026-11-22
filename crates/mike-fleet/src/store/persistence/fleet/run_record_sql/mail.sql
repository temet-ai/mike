WITH seats AS (
    SELECT rs.seat, rs.session_id AS sid, rs.launched_at, rs.kind
      FROM launch.run_seats rs
     WHERE rs.run_id = $1
)
SELECT f.seat AS "from_seat!", t.seat AS "to_seat!", count(*) AS "messages!",
       count(*) FILTER (WHERE m.channel_delivered_at IS NOT NULL OR m.sent_at IS NOT NULL) AS "reached!",
       count(*) FILTER (WHERE m.consumed_at IS NOT NULL) AS "acknowledged!"
  FROM (SELECT min(launched_at) AS at FROM seats) since
  JOIN mailbox.messages m ON m.queued_at >= since.at
  JOIN seats f ON f.sid = m.from_session_id
  JOIN seats t ON t.sid = m.to_session_id
 GROUP BY f.seat, t.seat
 ORDER BY count(*) DESC, f.seat, t.seat
