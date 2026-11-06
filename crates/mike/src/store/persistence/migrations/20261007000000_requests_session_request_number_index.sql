CREATE INDEX IF NOT EXISTS idx_requests_session_request_number
    ON requests.requests (session_id, request_number DESC)
    WHERE request_number IS NOT NULL;
