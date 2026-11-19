-- Idle re-sends (#433): after the prompt cache lapses Claude Code sends the whole conversation
-- again with thread {type: create}. Capture now reads a create request with three or more
-- messages, in a lane that already has a request, as a restart of that lane's conversation:
-- predecessor = the lane's latest request, message bodies already stored are reused, and only
-- messages new to the conversation get added delta rows.

INSERT INTO requests.predecessor_rules (id, code) VALUES (5, 'thread_restart')
ON CONFLICT (id) DO NOTHING;

COMMENT ON TABLE requests.predecessor_rules IS
    'How capture chose a request''s predecessor: thread_link (the request whose response carries the body''s thread.previous_message_id), thread_start (thread.type = create opening a lane: no predecessor), thread_restart (thread.type = create re-sending a lane''s conversation: the lane''s latest request), lane_fallback (latest earlier request in the same lane; a guess), none (no candidate).';
