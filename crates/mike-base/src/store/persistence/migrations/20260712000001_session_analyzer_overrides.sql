-- Per-session analyzer enable/disable overrides, backing
-- `PATCH /api/sessions/{id}/analyzers/{name}` (issue #174).
--
-- One row per (session, analyzer) override; absence of a row means the
-- analyzer's global config state applies unchanged. Mirrors the
-- `steering.enrolments` shape (session_id + a name key, UNIQUE pair) rather
-- than `sessions.session_goals` (1:1), since a session can override
-- multiple analyzers independently.

CREATE TABLE IF NOT EXISTS sessions.session_analyzer_overrides (
    id            bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    session_id    bigint NOT NULL
                    REFERENCES sessions.sessions(id) ON DELETE CASCADE,
    analyzer_name text NOT NULL,
    enabled       bool NOT NULL,
    updated_at    timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT uq_session_analyzer_overrides_session_analyzer
        UNIQUE (session_id, analyzer_name)
);

-- Session analyzer names (`weasel_intercept` + the 5 category analyzers)
-- are a fixed, small set defined in Rust (analyzer.rs), not a DB vocab
-- table — a text column matches how `requests.diagnostics.analyzer` and
-- `SessionAnalyzerDto.analyzer_name` already carry this identifier.
--
-- No separate session_id index: the UNIQUE constraint above is a
-- composite btree on (session_id, analyzer_name), which already serves
-- the "all overrides for this session" lookup via its leftmost column.
