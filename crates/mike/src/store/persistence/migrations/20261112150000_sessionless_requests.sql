-- Requests mike forwards that carry no canonical session id. Claude Code sends
-- several control-plane calls without x-claude-code-session-id: GET
-- /v1/mcp_servers, POST /v1/design/mcp, POST /v1/code/sessions (creating a
-- Remote Control channel) and GET /v1/ultrareview/quota. Until this table they
-- were forwarded and never stored; now each one is kept whole here.
CREATE TABLE requests.sessionless_requests (
    id                 BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    reason             TEXT NOT NULL CHECK (reason IN (
                           'missing_session_header',
                           'malformed_session_header',
                           'non_utf8_session_header')),
    method             TEXT NOT NULL,
    path               TEXT NOT NULL,
    query              TEXT,
    user_agent         TEXT,
    request_headers    JSONB NOT NULL,
    request_body       BYTEA NOT NULL,
    response_status    SMALLINT,
    response_headers   JSONB,
    response_body      BYTEA,
    response_body_size BIGINT,
    response_body_refused BOOLEAN NOT NULL DEFAULT false,
    status             TEXT NOT NULL CHECK (status IN ('complete', 'error', 'aborted')),
    error              TEXT,
    created_at         TIMESTAMPTZ NOT NULL,
    completed_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_sessionless_requests_path_created
    ON requests.sessionless_requests (path, created_at);

CREATE INDEX brin_sessionless_requests_created_at
    ON requests.sessionless_requests USING brin (created_at);
