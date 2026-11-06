WITH run_sessions AS (
    SELECT DISTINCT session_uuid
      FROM launch.launches
     WHERE run_id = 'fleet-release-journey-e8294cf4' AND session_uuid IS NOT NULL
), latest_launches AS (
    SELECT DISTINCT ON (l.session_uuid)
           l.session_uuid, l.run_id, l.closed_at, l.seat
      FROM launch.launches l
      JOIN run_sessions s USING (session_uuid)
     ORDER BY l.session_uuid, l.created_at DESC, l.id DESC
)
SELECT f.id, sender.session_uuid AS sender, recipient.session_uuid AS recipient,
       target.seat, f.attempted_at
  FROM mailbox.failed_sends f
  JOIN sessions.sessions sender ON sender.id = f.attempted_by_session_id
  JOIN latest_launches author ON author.session_uuid = sender.session_uuid
  JOIN sessions.sessions recipient ON recipient.session_uuid = sessions.try_uuid(f.attempted_to)
  JOIN latest_launches target ON target.session_uuid = recipient.session_uuid
 WHERE f.reason = 'recipient_ended'
   AND f.attempted_at >= now() - interval '30 days' AND f.attempted_at <= now()
   AND author.run_id = 'fleet-release-journey-e8294cf4' AND target.run_id = 'fleet-release-journey-e8294cf4'
   AND target.closed_at <= f.attempted_at
   AND NOT EXISTS (
       SELECT 1 FROM launch.told t
        WHERE t.run_id = 'fleet-release-journey-e8294cf4' AND t.what = 'closed-mail:' || f.id::text
   )
 ORDER BY f.attempted_at DESC, f.id DESC
 LIMIT 200
