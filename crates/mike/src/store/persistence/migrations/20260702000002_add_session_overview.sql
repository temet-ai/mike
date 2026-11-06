-- Session narrative overview: a two-paragraph prose summary of a session,
-- either LLM-generated or operator-authored. Distinct from the per-turn
-- stop-response digest served by GET /api/sessions/{id}/overview
-- (rehydrate.rs fetch_session_head / operator_turns) — that endpoint predates
-- this column pair and answers a different question ("what did each turn
-- say"), not "what happened in this session".
--
-- Tri-state invariant (also documented on PersistHandle::set_session_overview
-- in store/persistence/session_overview.rs):
--   overview NOT NULL, overview_generated_at NULL     -> operator-authored (manual override)
--   overview NOT NULL, overview_generated_at NOT NULL -> LLM-generated at that timestamp
--   overview NULL,     overview_generated_at NULL     -> no overview yet
--
-- Plain nullable columns, no index: neither is ever a filter/sort/join key
-- (a two-paragraph free-text field has no equality/range predicate use), so
-- no CREATE INDEX CONCURRENTLY step is needed here. Expand-only, matching
-- scripts/deploy-proxy.sh's migration contract.

ALTER TABLE sessions.sessions
    ADD COLUMN IF NOT EXISTS overview text NULL,
    ADD COLUMN IF NOT EXISTS overview_generated_at timestamptz NULL;

COMMENT ON COLUMN sessions.sessions.overview IS
    'Two-paragraph narrative overview of the session. NOT NULL with '
    'overview_generated_at NULL means operator-authored (manual override); '
    'NOT NULL with overview_generated_at set means LLM-generated at that '
    'timestamp; both NULL means no overview has been produced yet.';

COMMENT ON COLUMN sessions.sessions.overview_generated_at IS
    'Timestamp the LLM generated sessions.overview. NULL while overview is '
    'also NULL (no overview yet) or when overview was operator-authored '
    '(manual override, never LLM-generated) — see the column comment on '
    'sessions.sessions.overview for the full tri-state invariant.';

-- Extend session_summary_v (read-tier projection, 150ms standing exception
-- in .claude/rules/db-performance-evaluation.md) with the two new columns.
-- Every existing column is reissued unchanged and in the same position;
-- sqlx::FromRow decodes SessionSummaryRow by column name, not position, but
-- keeping the order stable avoids gratuitous diffs against prior migrations.
-- s.overview / s.overview_generated_at are functionally dependent on s.id
-- (already the GROUP BY key for the base table), so no GROUP BY change is
-- needed — same reasoning that already applies to s.title / s.pinned above.
CREATE OR REPLACE VIEW claude_proxy.session_summary_v AS
SELECT s.session_uuid::text                        AS session_id,
       p.project_path,
       s.first_seen,
       s.last_seen,
       s.last_status,
       s.title,
       s.pinned,
       p.pinned                                    AS project_pinned,
       COUNT(r.id)::int4                           AS request_count,
       COALESCE(mdl.last_models, ARRAY[]::text[]) AS last_models,
       sq.outcome                                  AS session_outcome,
       sq.quality_score                            AS session_quality_score,
       sq.judged_at                                AS session_judged_at,
       s.overview,
       s.overview_generated_at
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
LEFT JOIN claude_proxy.session_quality_v sq
       ON sq.session_id = s.session_uuid::text
GROUP BY s.id, p.id, mdl.last_models, sq.outcome, sq.quality_score, sq.judged_at;

COMMENT ON VIEW claude_proxy.session_summary_v IS
    'One row per session with its project-joined summary metadata. '
    'request_count via LEFT JOIN GROUP BY (one pass over requests). '
    'last_models: deduped list of models in chronological first-occurrence order '
    '(MIN(created_at) per model inside a LATERAL derived table — eliminates the '
    'correlated scalar subquery that previously ran once per session row on every '
    'session-list page load). '
    'last_path dropped (always /v1/messages). '
    'Latency computed on demand against idx_requests_session_completed. '
    'session_outcome / session_quality_score / session_judged_at: nullable columns '
    'sourced from claude_proxy.session_quality_v via LEFT JOIN — NULL when the '
    'session has not yet been judged by the LLM judge (GitHub issue #24). '
    'overview / overview_generated_at: the session narrative overview — NULL/NULL '
    'means none yet; see the column comments on sessions.sessions for the full '
    'tri-state (operator-authored vs LLM-generated) invariant.';
