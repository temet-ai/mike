-- Claude Code labels a running subagent with a side call (#434): one user message holding
-- <task>…</task><recent_steps>…</recent_steps> and "Describe the agent's most recent action in
-- 3-5 words". It carries the subagent's agent id and no thread, so it was classified `subagent`:
-- it joined the subagent's agent instance and took a lane-fallback predecessor in the subagent's
-- chain. It is now the side-call origin agent_label_sidecar.
--
-- This migration registers the origin and re-labels the existing rows: no instance, no
-- predecessor, author harness. Requests whose lane-fallback predecessor was a label call take
-- that call's own predecessor. Their delta rows are corrected by the thread-delta correction job
-- (#354), not here.

SET LOCAL lock_timeout = '10s';

INSERT INTO requests.turn_origins (id, code) OVERRIDING SYSTEM VALUE VALUES (13, 'agent_label_sidecar')
ON CONFLICT DO NOTHING;
SELECT setval('requests.turn_origins_id_seq', GREATEST(13, (SELECT max(id) FROM requests.turn_origins)));

CREATE TEMP TABLE agent_label_calls ON COMMIT DROP AS
SELECT r.id, r.predecessor_request_id
  FROM requests.requests r
 WHERE r.turn_origin_id = (SELECT id FROM requests.turn_origins WHERE code = 'subagent')
   AND r.message_count = 1
   AND r.last_user_message LIKE '<task>%'
   AND r.last_user_message LIKE '%Describe the agent''s most recent action%';

UPDATE requests.requests r
   SET predecessor_request_id = l.predecessor_request_id
  FROM agent_label_calls l
 WHERE r.predecessor_request_id = l.id
   AND r.id NOT IN (SELECT id FROM agent_label_calls);

UPDATE requests.requests r
   SET turn_origin_id = (SELECT id FROM requests.turn_origins WHERE code = 'agent_label_sidecar'),
       agent_instance_id = NULL,
       predecessor_request_id = NULL,
       predecessor_rule_id = (SELECT id FROM requests.predecessor_rules WHERE code = 'none'),
       author_kind_id = (SELECT id FROM requests.author_kinds WHERE code = 'harness')
  FROM agent_label_calls l
 WHERE r.id = l.id;
