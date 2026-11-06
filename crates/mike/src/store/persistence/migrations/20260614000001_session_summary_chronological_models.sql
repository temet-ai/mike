-- Fix session_summary_v: return last_models in chronological first-occurrence
-- order instead of alphabetical. The previous ARRAY_AGG(DISTINCT model ORDER
-- BY model) produced an alphabetical sort, but the UI and tests expect models
-- in the order the user first encountered each one across the session.
--
-- Implementation: a lateral subquery computes MIN(created_at) per model so we
-- can ORDER BY first_seen. DISTINCT is handled implicitly by the GROUP BY inside
-- the subquery; the outer ARRAY() then assembles them in first-seen order.
-- The view comment is updated to document the change.

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
       COALESCE(
           (
               SELECT ARRAY_AGG(model ORDER BY first_seen)
               FROM (
                   SELECT r2.model, MIN(r2.created_at) AS first_seen
                   FROM requests.requests r2
                   WHERE r2.session_id = s.id AND r2.model IS NOT NULL
                   GROUP BY r2.model
               ) sub
           ),
           ARRAY[]::text[]
       ) AS last_models
FROM sessions.sessions s
JOIN projects.projects p ON p.id = s.project_id
LEFT JOIN requests.requests r ON r.session_id = s.id
GROUP BY s.id, p.id;

COMMENT ON VIEW claude_proxy.session_summary_v IS
    'One row per session with its project-joined summary metadata. '
    'request_count via LEFT JOIN GROUP BY (one pass over requests). '
    'last_models: deduped list of models in chronological first-occurrence order '
    '(MIN(created_at) per model inside a correlated subquery so ORDER BY is stable). '
    'last_path dropped (always /v1/messages). '
    'Latency computed on demand against idx_requests_session_completed.';
