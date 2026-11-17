-- Lessons reach main sessions through the client (the mike mod or tether);
-- the proxy injects only into sessions whose client never asked for them.
--
-- memory.lesson_delivery_arms.last_call_at: when the session's client last
-- called POST /api/lessons/for-prompt. Startup rebuilds the set of client
-- sessions from it, so a restart never sends a client session back to proxy
-- injection. Old rows keep their arm, including 'control'.
--
-- memory.lesson_holdings: which lesson, at which revision, each session was
-- given (route 'mod' through the client, 'proxy' by mike's injection, 'body'
-- seen tagged in a request). The proxy never injects a lesson a session
-- holds at that revision; a client refresh (compaction, new session) clears
-- the session's rows.

ALTER TABLE memory.lesson_delivery_arms
    ADD COLUMN IF NOT EXISTS last_call_at timestamptz;

CREATE TABLE IF NOT EXISTS memory.lesson_holdings (
    session_uuid uuid        NOT NULL,
    lesson_id    bigint      NOT NULL REFERENCES memory.lessons(id) ON DELETE CASCADE,
    revision     integer     NOT NULL,
    route        text        NOT NULL CHECK (route IN ('mod', 'proxy', 'body')),
    held_at      timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (session_uuid, lesson_id)
);

CREATE INDEX IF NOT EXISTS idx_lesson_holdings_held_at ON memory.lesson_holdings (held_at DESC);

COMMENT ON TABLE memory.lesson_holdings IS 'Which lessons, at which revision, a session was given. Written by POST /api/lessons/for-prompt (route mod), by the session_lessons stage when it injects (proxy) or sees a lesson tag in a request (body). The session_lessons stage reads it so a held lesson is never sent again, across restarts.';
