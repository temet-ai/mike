-- no-transaction
-- Capture resolves a continue request's thread.previous_message_id to the request whose response
-- carries it (#350); without this index that lookup scanned responses.responses.
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_responses_upstream_message_id ON responses.responses (upstream_message_id) INCLUDE (request_id) WHERE upstream_message_id IS NOT NULL;
