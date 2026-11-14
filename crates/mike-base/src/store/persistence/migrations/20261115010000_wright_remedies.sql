-- Wright known fixes (temet-ai/mike#74): typed, bounded actions Wright runs when a check fails
-- and a known fix exists. Each action goes through the part of mike that owns the resource.
--
-- wright.remedies: one row per known fix. action is the typed action code the runner knows;
-- check_name is the check whose problems it answers. daily_limit and max_resources bound it;
-- three refusals or failures in a row pause it (paused_at, pause_reason); three ineffective
-- runs in a row turn it off (enabled false, with the reason in pause_reason). An ineffective
-- run is an action whose episode is still open measure_after_passes complete runs later.
-- wright.actions: one row per attempt. UNIQUE (remedy, episode_id) acts on an episode once,
-- whatever the number of workers or restarts. prior_state is read before the action; effect is
-- what a re-read found after it.
-- wright.remedy_changes: each operator change to a remedy's switch or limits.
-- wright.wright_session_uuids(): the sessions Wright itself starts, left out of every measure
-- Wright is judged on. Wright starts none until its engineer runs exist (temet-ai/mike#73),
-- whose migration replaces this body with the same signature.
-- wright.checks gains orphan_launches: a launch still open after its process was reaped.

SET LOCAL lock_timeout = '10s';

CREATE TABLE wright.remedies (
    name                 text        PRIMARY KEY CHECK (name ~ '^[a-z][a-z0-9_]{2,62}$'),
    kind                 text        NOT NULL CHECK (kind IN ('tidy', 'setting', 'notice')),
    action               text        NOT NULL CHECK (action IN ('close_orphan_launch', 'notify_undelivered_sender',
                                                                'set_setting', 'disable_filter_rule')),
    check_name           text        NOT NULL REFERENCES wright.checks(name) ON DELETE CASCADE,
    params               jsonb       NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(params) = 'object'),
    enabled              boolean     NOT NULL DEFAULT true,
    daily_limit          integer     NOT NULL CHECK (daily_limit BETWEEN 1 AND 1000),
    max_resources        integer     NOT NULL CHECK (max_resources BETWEEN 1 AND 1000),
    consecutive_refusals integer     NOT NULL DEFAULT 0 CHECK (consecutive_refusals >= 0),
    paused_at            timestamptz,
    pause_reason         text,
    measure_after_passes integer     NOT NULL DEFAULT 3 CHECK (measure_after_passes BETWEEN 1 AND 100),
    ineffective_runs     integer     NOT NULL DEFAULT 0 CHECK (ineffective_runs >= 0),
    created_at           timestamptz NOT NULL DEFAULT now(),
    updated_at           timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT remedies_pause_has_reason CHECK ((paused_at IS NULL) = (pause_reason IS NULL)),
    CONSTRAINT remedies_kind_matches_action CHECK (
        (action = 'close_orphan_launch' AND kind = 'tidy')
        OR (action = 'notify_undelivered_sender' AND kind = 'notice')
        OR (action IN ('set_setting', 'disable_filter_rule') AND kind = 'setting'))
);

CREATE INDEX idx_wright_remedies_check ON wright.remedies (check_name);

CREATE TABLE wright.actions (
    id           bigint      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    remedy       text        NOT NULL REFERENCES wright.remedies(name) ON DELETE CASCADE,
    problem_id   bigint      NOT NULL REFERENCES wright.problems(id) ON DELETE CASCADE,
    episode_id   bigint      NOT NULL REFERENCES wright.episodes(id) ON DELETE CASCADE,
    state        text        NOT NULL CHECK (state IN ('claimed', 'applied', 'verified', 'failed', 'refused',
                                                       'undone', 'undo_conflict', 'ineffective')),
    by           text        NOT NULL DEFAULT 'wright' CHECK (by IN ('wright', 'operator')),
    prior_state  jsonb,
    effect       jsonb,
    refusal      text,
    effective    boolean,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    applied_at   timestamptz,
    verified_at  timestamptz,
    measured_at  timestamptz,
    undone_at    timestamptz,
    CONSTRAINT actions_one_per_episode UNIQUE (remedy, episode_id),
    CONSTRAINT actions_refusal_has_reason CHECK (
        state NOT IN ('refused', 'failed', 'undo_conflict') OR refusal IS NOT NULL),
    CONSTRAINT actions_measured_has_verdict CHECK ((measured_at IS NULL) = (effective IS NULL))
);

CREATE INDEX idx_wright_actions_problem ON wright.actions (problem_id, id DESC);
CREATE INDEX idx_wright_actions_episode ON wright.actions (episode_id);
CREATE INDEX idx_wright_actions_remedy_created ON wright.actions (remedy, created_at DESC);
CREATE INDEX idx_wright_actions_unfinished ON wright.actions (updated_at)
    WHERE state IN ('claimed', 'applied');
CREATE INDEX idx_wright_actions_unmeasured ON wright.actions (verified_at)
    WHERE state = 'verified' AND measured_at IS NULL;

CREATE TABLE wright.remedy_changes (
    id         bigint      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    remedy     text        NOT NULL REFERENCES wright.remedies(name) ON DELETE CASCADE,
    changed_at timestamptz NOT NULL DEFAULT now(),
    by         text        NOT NULL CHECK (by IN ('operator', 'wright')),
    old_value  jsonb       NOT NULL,
    new_value  jsonb       NOT NULL
);

CREATE INDEX idx_wright_remedy_changes_remedy ON wright.remedy_changes (remedy, changed_at DESC);

COMMENT ON TABLE wright.remedies IS
    'Wright''s known fixes: one typed, bounded action per check it answers (temet-ai/mike#74).';
COMMENT ON TABLE wright.actions IS
    'One row per attempt of a known fix on one episode of a problem; at most one per remedy and episode.';

CREATE OR REPLACE FUNCTION wright.wright_session_uuids() RETURNS SETOF uuid
LANGUAGE sql STABLE AS $$
    SELECT NULL::uuid WHERE false
$$;

INSERT INTO wright.checks (name, source, cadence, expects, probe, query, params) VALUES
('orphan_launches', 'capture_db', 'regular',
 'No launch stays open 30 minutes after its process was reaped with no request since',
 'sql',
 $q$SELECT 'launch'::text AS scope,
       l.id::text AS resource,
       jsonb_build_object(
           'launch_id', l.id,
           'session_uuid', l.session_uuid,
           'run_id', l.run_id,
           'seat', l.seat,
           'backend', l.backend,
           'launched_at', l.created_at,
           'reaped_at', r.reaped_at,
           'session_last_seen', s.last_seen
       ) AS evidence
  FROM launch.launches l
  JOIN sessions.sessions s ON s.session_uuid = l.session_uuid
  JOIN workspace.session_registrations r ON r.session_id = s.id
 WHERE l.closed_at IS NULL
   AND r.reaped_at IS NOT NULL
   AND r.reaped_at < now() - make_interval(mins => ($1->>'min_gone_minutes')::int)
   AND s.last_seen <= r.reaped_at
   AND l.created_at >= now() - make_interval(days => ($1->>'window_days')::int)
 ORDER BY l.id
 LIMIT 200$q$,
 '{"min_gone_minutes": 30, "window_days": 14}')
ON CONFLICT (name) DO NOTHING;

INSERT INTO wright.remedies (name, kind, action, check_name, daily_limit, max_resources, measure_after_passes)
SELECT v.name, v.kind, v.action, v.check_name, v.daily_limit, v.max_resources, v.measure_after_passes
  FROM (VALUES
        ('tell_undelivered_senders', 'notice', 'notify_undelivered_sender', 'undelivered_mail', 20, 20, 3),
        ('close_orphan_launches', 'tidy', 'close_orphan_launch', 'orphan_launches', 20, 1, 2)
       ) AS v(name, kind, action, check_name, daily_limit, max_resources, measure_after_passes)
  JOIN wright.checks c ON c.name = v.check_name
ON CONFLICT (name) DO NOTHING;
