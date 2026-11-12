-- no-transaction
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_runtime_responses_expiry
    ON runtime.responses_exchanges (expires_at) WHERE evidence_status = 'retained';
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_runtime_responses_metadata_expiry
    ON runtime.responses_exchanges (metadata_expires_at);
