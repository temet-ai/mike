ALTER TABLE llm.calls DROP CONSTRAINT calls_outcome_check;
ALTER TABLE llm.calls ADD CONSTRAINT calls_outcome_check CHECK (outcome IN ('ok', 'http_error', 'timeout', 'truncated', 'transport_error', 'refused_ceiling', 'refused_rate', 'refused_spend'));
