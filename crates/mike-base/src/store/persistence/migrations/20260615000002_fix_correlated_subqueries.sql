-- Fix all confirmed correlated scalar subqueries identified in the Phase 13
-- SQL audit. Applied in one migration for atomicity.
--
-- VIOLATION 1 (HOT-PATH): session_summary_v — correlated scalar subquery per
-- session row on every session-list page load. Replaced with LEFT JOIN LATERAL
-- whose aggregate resolves in one grouped pass, then assembled into an array.
--
-- VIOLATIONS 2–4 are in Rust (rehydrate.rs, compositions.rs) — fixed in those
-- files; no DDL needed here.
--
-- Existing view defined in 20260614000001_session_summary_chronological_models.sql.

CREATE OR REPLACE VIEW claude_proxy.session_summary_v AS
SELECT s.session_uuid::text AS session_id,
       p.project_path,
       s.first_seen,
       s.last_seen,
       s.last_status,
       s.title,
       s.pinned,
       p.pinned AS project_pinned,
       COUNT(r.id)::int4 AS request_count,
       COALESCE(mdl.last_models, ARRAY[]::text[]) AS last_models
FROM sessions.sessions s
JOIN projects.projects p ON p.id = s.project_id
LEFT JOIN requests.requests r ON r.session_id = s.id
LEFT JOIN LATERAL (
    SELECT ARRAY_AGG(model ORDER BY first_seen) AS last_models
    FROM (
        SELECT r2.model, MIN(r2.created_at) AS first_seen
        FROM requests.requests r2
        WHERE r2.session_id = s.id
          AND r2.model IS NOT NULL
        GROUP BY r2.model
    ) sub
) mdl ON true
GROUP BY s.id, p.id, mdl.last_models;

COMMENT ON VIEW claude_proxy.session_summary_v IS
    'One row per session with its project-joined summary metadata. '
    'request_count via LEFT JOIN GROUP BY (one pass over requests). '
    'last_models: deduped list of models in chronological first-occurrence order '
    '(MIN(created_at) per model inside a LATERAL derived table — eliminates the '
    'correlated scalar subquery that previously ran once per session row on every '
    'session-list page load). '
    'last_path dropped (always /v1/messages). '
    'Latency computed on demand against idx_requests_session_completed.';
