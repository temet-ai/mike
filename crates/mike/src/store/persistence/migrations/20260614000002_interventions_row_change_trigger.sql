-- Phase 19 steering Phase 6: LISTEN/NOTIFY trigger on steering.interventions.
--
-- Follows the same pattern as the four existing triggers in the baseline
-- (20260604000001_baseline.sql lines 1144–1211). Payload shape:
--   { "table": "interventions", "id": "<bigint>", "op": "INSERT"|"UPDATE" }
--
-- The `session_id` field is intentionally omitted: the SPA reacts by
-- refreshing the per-session steering panel which is already keyed to a
-- session UUID; the request panel (BigbroAttribution) reacts directly to
-- the `SteeringFired` ProxyEvent emitted by the fleet task (which carries
-- both `session_id` and `trigger_request_id`). The listener task just needs
-- to know a row changed so cross-session SPA tabs can invalidate their
-- caches. Keeping the payload minimal matches the filter_rules and
-- prompt_compositions precedents (session-independent tables).

CREATE FUNCTION claude_proxy.notify_row_changed_interventions()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table', 'interventions',
        'id',    COALESCE(NEW.id, OLD.id)::text,
        'op',    TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE TRIGGER trg_interventions_row_changed
AFTER INSERT OR UPDATE OR DELETE ON steering.interventions
FOR EACH ROW EXECUTE FUNCTION claude_proxy.notify_row_changed_interventions();
