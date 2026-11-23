CREATE TABLE IF NOT EXISTS runtime.capture_documents (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    predecessor_id bigint REFERENCES runtime.capture_documents(id) ON DELETE SET NULL,
    byte_size bigint NOT NULL DEFAULT 0 CHECK (byte_size >= 0),
    chunk_count bigint NOT NULL DEFAULT 0 CHECK (chunk_count >= 0),
    finished boolean NOT NULL DEFAULT false,
    complete boolean NOT NULL DEFAULT false,
    created_at timestamptz NOT NULL DEFAULT now(),
    CHECK (NOT complete OR finished)
);
CREATE INDEX IF NOT EXISTS idx_capture_documents_predecessor ON runtime.capture_documents (predecessor_id);

CREATE TABLE IF NOT EXISTS runtime.capture_chunks (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    origin_document_id bigint REFERENCES runtime.capture_documents(id) ON DELETE SET NULL,
    origin_ordinal bigint NOT NULL CHECK (origin_ordinal >= 0),
    body bytea NOT NULL,
    byte_size bigint GENERATED ALWAYS AS (octet_length(body)::bigint) STORED,
    UNIQUE (origin_document_id, origin_ordinal)
);

CREATE TABLE IF NOT EXISTS runtime.capture_document_chunks (
    document_id bigint NOT NULL REFERENCES runtime.capture_documents(id) ON DELETE CASCADE,
    ordinal bigint NOT NULL CHECK (ordinal >= 0),
    chunk_id bigint NOT NULL REFERENCES runtime.capture_chunks(id) ON DELETE RESTRICT,
    PRIMARY KEY (document_id, ordinal)
);
CREATE INDEX IF NOT EXISTS idx_capture_document_chunks_chunk ON runtime.capture_document_chunks (chunk_id);

CREATE TABLE IF NOT EXISTS runtime.capture_exchanges (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    exchange_uuid uuid NOT NULL UNIQUE,
    session_id bigint REFERENCES sessions.sessions(id) ON DELETE RESTRICT,
    provider_thread_id text,
    created_at timestamptz NOT NULL,
    metadata jsonb NOT NULL CHECK (jsonb_typeof(metadata) = 'object'),
    request_document_id bigint NOT NULL REFERENCES runtime.capture_documents(id) ON DELETE RESTRICT,
    sent_document_id bigint NOT NULL REFERENCES runtime.capture_documents(id) ON DELETE RESTRICT,
    response_document_id bigint REFERENCES runtime.capture_documents(id) ON DELETE RESTRICT,
    status text NOT NULL CHECK (status IN ('pending', 'complete', 'error', 'aborted', 'refused'))
);
COMMENT ON COLUMN runtime.capture_exchanges.metadata IS
    'Complete provider wire metadata, including unknown header and protocol fields; query keys have typed columns.';
CREATE INDEX IF NOT EXISTS idx_capture_exchanges_session ON runtime.capture_exchanges (session_id, created_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_capture_exchanges_thread ON runtime.capture_exchanges (provider_thread_id, created_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_capture_exchanges_request ON runtime.capture_exchanges (request_document_id);
CREATE INDEX IF NOT EXISTS idx_capture_exchanges_sent ON runtime.capture_exchanges (sent_document_id);
CREATE INDEX IF NOT EXISTS idx_capture_exchanges_response ON runtime.capture_exchanges (response_document_id);
