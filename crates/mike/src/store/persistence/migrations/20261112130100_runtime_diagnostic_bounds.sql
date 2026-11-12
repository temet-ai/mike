ALTER TABLE runtime.responses_exchanges
    ALTER COLUMN request_body DROP NOT NULL,
    ALTER COLUMN forwarded_body DROP NOT NULL,
    ADD COLUMN IF NOT EXISTS evidence_status text NOT NULL DEFAULT 'retained'
        CHECK (evidence_status IN ('not_collected', 'retained', 'budget_refused', 'incomplete', 'expired')),
    ADD COLUMN IF NOT EXISTS expires_at timestamptz NOT NULL DEFAULT (now() + interval '1 day'),
    ADD COLUMN IF NOT EXISTS metadata_expires_at timestamptz NOT NULL DEFAULT (now() + interval '7 days'),
    ADD COLUMN IF NOT EXISTS retained_bytes bigint GENERATED ALWAYS AS (
        COALESCE(octet_length(request_body), 0) + COALESCE(octet_length(forwarded_body), 0)
        + COALESCE(octet_length(response_body), 0)
        + octet_length(request_headers::text) + octet_length(forwarded_headers::text)
        + COALESCE(octet_length(response_headers::text), 0)
        + octet_length(correlation::text) + COALESCE(octet_length(error), 0)
        + octet_length(path) + octet_length(upstream_url)
        + COALESCE(octet_length(provider_session_id), 0) + COALESCE(octet_length(provider_thread_id), 0)
        + COALESCE(octet_length(provider_turn_id), 0) + COALESCE(octet_length(request_kind), 0)
        + octet_length(auth_route) + octet_length(transport) + octet_length(method)
        + COALESCE(octet_length(request_body_encoding), 0) + COALESCE(octet_length(model), 0)
        + COALESCE(octet_length(upstream_request_id), 0) + COALESCE(octet_length(response_id), 0)
        + COALESCE(octet_length(response_model), 0) + COALESCE(octet_length(response_state), 0)
        + COALESCE(octet_length(usage::text), 0) + COALESCE(octet_length(output_items::text), 0)
        + octet_length(status) + octet_length(evidence_status) + 1024
    ) STORED;

UPDATE runtime.storage_accounting
   SET retained_rows = totals.rows, retained_bytes = totals.bytes
  FROM (SELECT count(*) AS rows, COALESCE(sum(retained_bytes), 0) AS bytes
          FROM runtime.responses_exchanges) totals
 WHERE kind = 'diagnostics';

CREATE OR REPLACE TRIGGER account_runtime_diagnostics
AFTER INSERT OR UPDATE OR DELETE ON runtime.responses_exchanges
FOR EACH ROW EXECUTE FUNCTION runtime.account_observation_storage('diagnostics');
