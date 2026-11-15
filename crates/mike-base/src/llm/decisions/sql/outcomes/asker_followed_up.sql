WITH linked AS (
    SELECT r.id AS record_id, r.created_at,
           m.id AS message_id, m.from_session_id AS asker_id, m.to_session_id AS replier_id
      FROM decisions.records r
      JOIN mailbox.messages m
        ON m.message_uuid = (r.composed -> 'verdict' ->> 'linked_message')::uuid
     WHERE r.point = 'mail_reply_link'
       AND r.applied = 'chosen'
       AND r.created_at >= now() - interval '30 days'
       AND r.composed -> 'verdict' ->> 'linked_message' IS NOT NULL
),
replies AS (
    SELECT DISTINCT ON (l.record_id)
           l.record_id, l.asker_id, l.replier_id,
           x.id AS reply_id, x.thread_id AS reply_thread, x.queued_at AS reply_at
      FROM linked l
      JOIN mailbox.messages x
        ON x.in_reply_to_id = l.message_id
       AND x.from_session_id = l.replier_id
       AND x.to_session_id = l.asker_id
       AND x.queued_at >= l.created_at
       AND x.queued_at < l.created_at + interval '5 minutes'
     ORDER BY l.record_id, x.queued_at, x.id
),
later AS (
    SELECT p.record_id,
           bool_or(COALESCE(f.in_reply_to_id = p.reply_id, false)
                   OR COALESCE(f.thread_id = p.reply_thread, false)) AS followed_up,
           bool_or(f.queued_at <= p.reply_at + interval '1 hour'
                   AND f.in_reply_to_id IS NULL
                   AND (p.reply_thread IS NULL OR f.thread_id IS DISTINCT FROM p.reply_thread)) AS asked_again
      FROM replies p
      JOIN mailbox.messages f
        ON f.from_session_id = p.asker_id
       AND f.to_session_id = p.replier_id
       AND f.queued_at > p.reply_at
       AND f.queued_at <= p.reply_at + interval '24 hours'
     GROUP BY p.record_id
)
INSERT INTO decisions.outcomes (record_id, label, label_source, evidence)
SELECT l.record_id,
       CASE WHEN l.followed_up THEN 'right' ELSE 'wrong' END,
       'asker_followed_up',
       jsonb_build_object('followed_up_in_thread', l.followed_up, 'asked_again', l.asked_again)
  FROM later l
 WHERE (l.followed_up OR l.asked_again)
   AND NOT EXISTS (SELECT 1 FROM decisions.outcomes o
                    WHERE o.record_id = l.record_id AND o.label_source = 'asker_followed_up')
ON CONFLICT (record_id, label_source, label) DO NOTHING
