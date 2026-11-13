CREATE TABLE runtime.responses_exchanges (
    id                    BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    session_id            BIGINT REFERENCES sessions.sessions(id) ON DELETE CASCADE,
    provider_session_id   TEXT,
    provider_thread_id    TEXT,
    provider_turn_id      TEXT,
    request_kind          TEXT,
    correlation           JSONB NOT NULL DEFAULT '{}'::jsonb,
    auth_route            TEXT NOT NULL CHECK (auth_route IN ('chatgpt', 'api_key', 'none')),
    transport             TEXT NOT NULL CHECK (transport IN ('http', 'websocket_refused')),
    method                TEXT NOT NULL,
    path                  TEXT NOT NULL,
    upstream_url          TEXT NOT NULL,
    request_headers       JSONB NOT NULL,
    forwarded_headers     JSONB NOT NULL,
    request_body          BYTEA NOT NULL,
    forwarded_body        BYTEA NOT NULL,
    request_body_encoding TEXT,
    model                 TEXT,
    response_status       SMALLINT,
    response_headers      JSONB,
    response_body         BYTEA,
    upstream_request_id   TEXT,
    response_id           TEXT,
    response_model        TEXT,
    response_state        TEXT,
    usage                 JSONB,
    output_items          JSONB NOT NULL DEFAULT '[]'::jsonb,
    stream_events         INTEGER,
    error                 TEXT,
    status                TEXT NOT NULL CHECK (status IN ('complete', 'error', 'aborted', 'refused')),
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at          TIMESTAMPTZ,
    CONSTRAINT responses_exchanges_output_items_array CHECK (jsonb_typeof(output_items) = 'array')
);

CREATE INDEX idx_responses_exchanges_session_created
    ON runtime.responses_exchanges (session_id, created_at)
    WHERE session_id IS NOT NULL;

CREATE INDEX idx_responses_exchanges_thread_created
    ON runtime.responses_exchanges (provider_thread_id, created_at)
    WHERE provider_thread_id IS NOT NULL;

CREATE INDEX brin_responses_exchanges_created_at
    ON runtime.responses_exchanges USING brin (created_at);
