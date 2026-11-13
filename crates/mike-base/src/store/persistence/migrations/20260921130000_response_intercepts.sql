-- Durable evidence that the response-replacing paths ran (issue #318).
--
-- Before this table the only proof that a weasel intercept fired was a log
-- line: the flag on requests.requests is written by an UPDATE that matches no
-- row when the original request never persisted (stub insert failed and the
-- request carried a fallback id), and a response that was never scanned left
-- no trace at all. One row here per decision, keyed by session so it survives
-- a request row that does not exist, and by request when it does.
--
-- outcome:
--   weasel_correction     the streamed answer was discarded and a correction
--                         request was sent; subject is the forbidden word
--   proxy_tool_answer     the streamed answer was discarded and the proxy
--                         answered a proxy-declared tool call; subject is the
--                         tool name
--   not_scanned_buffered  weasel intercept was armed for the session but the
--                         upstream response was not an event stream, so no
--                         scan ran; subject is NULL

CREATE TABLE requests.response_intercepts (
    id          bigint      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    session_id  bigint      NOT NULL REFERENCES sessions.sessions(id) ON DELETE CASCADE,
    request_id  bigint      REFERENCES requests.requests(id) ON DELETE SET NULL,
    outcome     text        NOT NULL
                    CHECK (outcome IN (
                        'weasel_correction',
                        'proxy_tool_answer',
                        'not_scanned_buffered'
                    )),
    subject     text,
    created_at  timestamptz NOT NULL DEFAULT now()
);

COMMENT ON COLUMN requests.response_intercepts.request_id IS
    'The request whose response was replaced or left unscanned. NULL when that request was never persisted (fallback id) or has since been deleted; session_id and created_at still place the event.';

CREATE INDEX idx_response_intercepts_session_created
    ON requests.response_intercepts (session_id, created_at DESC);
CREATE INDEX idx_response_intercepts_request
    ON requests.response_intercepts (request_id) WHERE request_id IS NOT NULL;
CREATE INDEX brin_response_intercepts_created_at
    ON requests.response_intercepts USING BRIN (created_at);
