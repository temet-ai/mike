-- memory.search_events records what each project-knowledge search and each recall_similar_turns
-- call returned, in order with its scores, so that a later pass can mark which references the
-- calling session went on to use. One row per call. session_id is NULL when the caller did not
-- identify itself; request_id is the request whose response emitted the calling tool_use (or the
-- session's newest request when that tool_use is not captured yet).

CREATE TABLE memory.search_events (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    session_id bigint REFERENCES sessions.sessions(id) ON DELETE CASCADE,
    request_id bigint REFERENCES requests.requests(id) ON DELETE SET NULL,
    tool text NOT NULL,
    query text NOT NULL,
    kind_filter text,
    returned jsonb NOT NULL DEFAULT '[]'::jsonb,
    rerank_state text NOT NULL,
    used_references text[] NOT NULL DEFAULT '{}',
    used_final_at timestamptz,
    used_checked_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT search_events_tool_check
        CHECK (tool IN ('search_project_knowledge', 'recall_similar_turns')),
    CONSTRAINT search_events_rerank_state_check
        CHECK (rerank_state IN ('reranked', 'loading', 'off', 'unavailable')),
    CONSTRAINT search_events_returned_is_array_check
        CHECK (jsonb_typeof(returned) = 'array')
);

CREATE INDEX idx_search_events_session ON memory.search_events (session_id)
    WHERE session_id IS NOT NULL;

CREATE INDEX idx_search_events_request ON memory.search_events (request_id)
    WHERE request_id IS NOT NULL;

CREATE INDEX idx_search_events_open ON memory.search_events (used_checked_at NULLS FIRST, created_at)
    WHERE used_final_at IS NULL AND session_id IS NOT NULL AND request_id IS NOT NULL;

COMMENT ON TABLE memory.search_events IS 'One row per search_project_knowledge or recall_similar_turns call: the query, the references returned in order with their scores, and how the reranker took part. Written off the response path by the handlers; used_references and used_final_at are filled by the search_events worker.';
COMMENT ON COLUMN memory.search_events.session_id IS 'FK -> sessions.sessions(id) of the calling session; NULL when the caller sent no session id or the id is unknown. ON DELETE CASCADE.';
COMMENT ON COLUMN memory.search_events.request_id IS 'FK -> requests.requests(id) of the request whose response emitted the calling tool_use, else the session''s newest request when the call was recorded; NULL for a NULL session. ON DELETE SET NULL.';
COMMENT ON COLUMN memory.search_events.returned IS 'JSON array, in the order the caller saw it: {reference, score, rerank_score?}. score is the plain fused rank (knowledge) or rank (turns); rerank_score is present only on reranked results.';
COMMENT ON COLUMN memory.search_events.rerank_state IS 'reranked | loading | off | unavailable, as in the answer''s rerank.state.';
COMMENT ON COLUMN memory.search_events.used_references IS 'Returned references the same session later read with read_project_knowledge or get_operator_turn, or named in a final reply, from this request through the fifth following operator turn. Re-evaluated until used_final_at is set.';
COMMENT ON COLUMN memory.search_events.used_checked_at IS 'When the search_events worker last evaluated the used mark; the worker takes the open events checked longest ago first, so a large backlog cannot starve newer events.';
COMMENT ON COLUMN memory.search_events.used_final_at IS 'Set when the five following turns have closed or the event is a day old; the used mark no longer changes after that.';
