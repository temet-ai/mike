-- Capture follows Claude Code's thread link (#350) and gives every subagent request an agent
-- instance when it arrives (#351).
--
-- Since Claude Code 2.1.278 a request may carry thread {type: create|continue,
-- previous_message_id}; a continue body holds only the messages new since that response. The
-- predecessor of such a request is the request whose response carries previous_message_id.
-- requests.requests.predecessor_rule_id records which rule chose predecessor_request_id, so an
-- exact link can be told from a guess. Rows written before this migration keep NULL.
--
-- agents.instances.dispatch_id becomes nullable: capture creates the instance on a subagent's
-- first request, and the post-turn job links the Agent dispatch to it afterwards. Workflow
-- subagents have no Agent dispatch at all.
--
-- Every change is additive: a binary older than this migration keeps serving.

SET LOCAL lock_timeout = '10s';

CREATE TABLE IF NOT EXISTS requests.predecessor_rules (
    id smallint PRIMARY KEY,
    code text NOT NULL UNIQUE
);

INSERT INTO requests.predecessor_rules (id, code) VALUES
    (1, 'thread_link'),
    (2, 'thread_start'),
    (3, 'lane_fallback'),
    (4, 'none')
ON CONFLICT (id) DO NOTHING;

COMMENT ON TABLE requests.predecessor_rules IS
    'How capture chose a request''s predecessor: thread_link (the request whose response carries the body''s thread.previous_message_id), thread_start (thread.type = create: no predecessor), lane_fallback (latest earlier request in the same lane; a guess), none (no candidate).';

ALTER TABLE requests.requests
    ADD COLUMN IF NOT EXISTS predecessor_rule_id smallint REFERENCES requests.predecessor_rules(id);

COMMENT ON COLUMN requests.requests.predecessor_rule_id IS
    'Which rule chose predecessor_request_id (requests.predecessor_rules). NULL on rows captured before 2026-11-17.';

ALTER TABLE agents.instances ALTER COLUMN dispatch_id DROP NOT NULL;

COMMENT ON COLUMN agents.instances.dispatch_id IS
    'FK to the agents.dispatches row that spawned this instance. NULL until the post-turn job links the Agent dispatch to an instance capture created, and always NULL for workflow subagents.';
