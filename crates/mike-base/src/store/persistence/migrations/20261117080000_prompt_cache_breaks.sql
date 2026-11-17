-- Prompt-cache breaks: mike notices every unexpected cache miss on a main-thread request and
-- names its cause, so nobody has to query for it.
--
-- prompt_cache.breaks holds one row per main-thread request that rewrote cache it should have
-- read. kind 'break': the previous main request wrote cache whose TTL had not run out, and this
-- one read clearly less of it. kind 'expiry': the TTL had run out and the resume rewrote the
-- prefix (woken_by says what resumed it: the operator, mail, a hook). cause names who changed
-- the sent bytes: a mike stage (stage), the client (Claude Code changed its system prompt, tools
-- or history), unknown, or expired. location is the first place the sent bytes differ
-- (system, tools, messages_prefix, delta) and source_request_id the request that sent the
-- differing bytes.
--
-- prompt_cache.scan_state is the scanner's cursor over responses.responses ids. The first scan
-- starts seven days back, which is the backfill.
--
-- The Wright check prompt_cache_breaks opens a problem for each mike stage that caused at least
-- min_breaks breaks in window_hours, so a repeating stage reaches Wright's engineer.
--
-- Every change is additive: a binary older than this migration keeps serving.

SET LOCAL lock_timeout = '10s';

CREATE SCHEMA IF NOT EXISTS prompt_cache;

CREATE TABLE IF NOT EXISTS prompt_cache.breaks (
    id                BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    request_id        BIGINT      NOT NULL UNIQUE REFERENCES requests.requests(id) ON DELETE CASCADE,
    session_id        BIGINT      NOT NULL REFERENCES sessions.sessions(id) ON DELETE CASCADE,
    prev_request_id   BIGINT      REFERENCES requests.requests(id) ON DELETE SET NULL,
    source_request_id BIGINT      REFERENCES requests.requests(id) ON DELETE SET NULL,
    kind              TEXT        NOT NULL CHECK (kind IN ('break', 'expiry')),
    cause             TEXT        NOT NULL CHECK (cause IN ('mike', 'client', 'unknown', 'expired')),
    stage             TEXT,
    location          TEXT,
    woken_by          TEXT,
    ttl_seconds       INTEGER     NOT NULL,
    gap_seconds       INTEGER     NOT NULL,
    expected_read     BIGINT      NOT NULL,
    cache_read        BIGINT      NOT NULL,
    tokens_rewritten  BIGINT      NOT NULL,
    detail            JSONB       NOT NULL DEFAULT '{}'::jsonb,
    occurred_at       TIMESTAMPTZ NOT NULL,
    detected_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT breaks_stage_only_for_mike CHECK ((cause = 'mike') = (stage IS NOT NULL))
);

CREATE INDEX IF NOT EXISTS prompt_cache_breaks_session ON prompt_cache.breaks (session_id, occurred_at DESC);
CREATE INDEX IF NOT EXISTS prompt_cache_breaks_occurred ON prompt_cache.breaks (occurred_at DESC);

COMMENT ON TABLE prompt_cache.breaks IS
    'Main-thread requests that rewrote prompt cache they should have read (break) or that resumed after the TTL ran out (expiry), with the cause: a mike stage, the client, unknown or expired.';

CREATE TABLE IF NOT EXISTS prompt_cache.scan_state (
    id               SMALLINT    PRIMARY KEY CHECK (id = 1),
    last_response_id BIGINT      NOT NULL,
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO wright.checks (name, source, cadence, expects, probe, query, params) VALUES
('prompt_cache_breaks', 'capture_db', 'regular',
 'No mike stage makes Claude Code''s full re-sends miss the prompt cache: fewer than min_breaks breaks caused by one stage in the last window_hours',
 'sql',
 $q$SELECT 'stage'::text AS scope,
       b.stage AS resource,
       jsonb_build_object(
           'breaks', count(*),
           'sessions', count(DISTINCT b.session_id),
           'tokens_rewritten', sum(b.tokens_rewritten),
           'locations', jsonb_agg(DISTINCT b.location),
           'latest_request_id', max(b.request_id),
           'window_hours', ($1->>'window_hours')::int
       ) AS evidence
  FROM prompt_cache.breaks b
 WHERE b.cause = 'mike'
   AND b.occurred_at >= now() - make_interval(hours => ($1->>'window_hours')::int)
 GROUP BY b.stage
HAVING count(*) >= ($1->>'min_breaks')::int$q$,
 '{"window_hours": 24, "min_breaks": 3}'::jsonb)
ON CONFLICT (name) DO NOTHING;
