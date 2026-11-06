-- Add trigger_request_id to claude_proxy.steered_requests_v.
--
-- GET /api/requests/{id}/steering (per-request attribution panel,
-- BigbroAttribution.svelte) needs to link the delivered request back to
-- the request that triggered the steering evaluation. The trigger and the
-- sent/delivered request are always in the same session (a steerer watches
-- one session and injects into its own next eligible turn), so the column
-- is enough to build the "Trigger: #N ->" deep link without an extra join
-- on the frontend.
--
-- CREATE OR REPLACE VIEW requires every existing column to keep its name,
-- type, and ordinal position — new columns may only be appended at the
-- end. trigger_request_id is therefore added last, not alongside the
-- other i.* columns further up the SELECT list.
CREATE OR REPLACE VIEW claude_proxy.steered_requests_v AS
SELECT
    i.sent_request_id          AS request_id,
    i.sent_response_id         AS response_id,
    r.session_id,
    s.session_uuid,
    r.operator_turn_id,
    st.name                    AS steerer_name,
    st.display_name            AS steerer_display_name,
    auth.code                  AS authority,
    md.code                    AS mode,
    v.code                     AS verdict,
    i.off_thread_topic,
    i.confidence,
    i.payload_text,
    i.anchor_text,
    i.verdict_json,
    i.model,
    i.sent_at,
    i.processed_at,
    i.expires_at,
    i.reverted_at,
    i.revert_reason,
    i.created_at,
    i.trigger_request_id
FROM steering.interventions i
JOIN requests.requests r       ON r.id = i.sent_request_id
JOIN sessions.sessions s       ON s.id = i.session_id
JOIN steering.steerers st      ON st.id = i.steerer_id
JOIN steering.authorities auth ON auth.id = i.authority_id
JOIN steering.modes md         ON md.id = i.mode_id
JOIN steering.verdicts v       ON v.id = i.verdict_id
WHERE i.sent_request_id IS NOT NULL;
