-- repeated_messages: the capture should store each message a session sends once. From July to
-- 2026-11-24 a full resend whose guessed predecessor was a Claude Code continuation stored the
-- whole conversation again, and new threads and side calls stored the session's opening
-- messages again; about three in four rows of the newest million were repeats and nothing
-- reported it. This check counts, over window_hours of newly stored messages, the copies of a
-- message the same session had already stored in that window, grouped by the rule that linked
-- the storing request to its predecessor, and reports a rule whose repeats reach min_repeats
-- and max_repeat_share of what it stored.
INSERT INTO wright.checks (name, source, cadence, expects, probe, query, params) VALUES
('repeated_messages', 'capture_db', 'regular',
 'A session stores each message it sends once: a message already stored for the session is reused, whichever rule linked the request to the one before it',
 'sql',
 $q$WITH recent AS (
    SELECT m.id, m.message_sha256, m.byte_size
      FROM messages.messages m
     WHERE m.last_seen >= now() - make_interval(hours => ($1->>'window_hours')::int)
),
first_use AS (
    SELECT DISTINCT ON (rm.message_id) rm.message_id, rm.request_id
      FROM requests.messages rm
      JOIN recent ON recent.id = rm.message_id
     ORDER BY rm.message_id, rm.request_id
),
copies AS (
    SELECT recent.id, recent.message_sha256, recent.byte_size, q.session_id, q.id AS request_id,
           coalesce(pr.code, 'none') AS rule,
           row_number() OVER (PARTITION BY q.session_id, recent.message_sha256 ORDER BY recent.id) AS copy
      FROM recent
      JOIN first_use f ON f.message_id = recent.id
      JOIN requests.requests q ON q.id = f.request_id
      LEFT JOIN requests.predecessor_rules pr ON pr.id = q.predecessor_rule_id
),
grouped AS (
    SELECT rule,
           count(*) AS stored,
           count(*) FILTER (WHERE copy > 1) AS repeats,
           coalesce(sum(byte_size) FILTER (WHERE copy > 1), 0) AS repeat_bytes,
           count(DISTINCT session_id) FILTER (WHERE copy > 1) AS sessions,
           (array_agg(request_id ORDER BY id DESC) FILTER (WHERE copy > 1))[1] AS last_request_id
      FROM copies
     GROUP BY rule
)
SELECT 'message_store'::text AS scope,
       'repeated messages: ' || rule AS resource,
       jsonb_build_object(
           'window', ($1->>'window_hours') || ' hours',
           'predecessor_rule', rule,
           'stored', stored,
           'repeats', repeats,
           'repeat_share', round(repeats::numeric / greatest(stored, 1), 3),
           'repeat_bytes', repeat_bytes,
           'sessions', sessions,
           'last_request_id', last_request_id
       ) AS evidence
  FROM grouped
 WHERE repeats >= ($1->>'min_repeats')::int
   AND repeats::numeric / greatest(stored, 1) >= ($1->>'max_repeat_share')::numeric
 ORDER BY repeats DESC, resource
 LIMIT 200$q$,
 '{"window_hours": 6, "min_repeats": 50, "max_repeat_share": 0.1}')
ON CONFLICT (name) DO UPDATE
   SET expects = EXCLUDED.expects,
       query = EXCLUDED.query,
       params = EXCLUDED.params || wright.checks.params,
       updated_at = now();
