-- session_judges.attempts: durable record that the session judge called the LLM
-- for a session at a given model + rubric.
--
-- Why: fetch_sessions_needing_judgment re-selected any session whose overview
-- was NULL, and the judge writes no overview for an `unknown` outcome, so those
-- sessions were judged again on every poll with nothing recorded that an attempt
-- had happened. The selection query now consults this table: a session is only
-- re-selected when it has never been attempted at the current model + rubric,
-- has operator turns newer than last_attempt_at, or its last attempt failed and
-- the exponential backoff on consecutive_failures has elapsed.
--
-- Grain: one row per (session, judge_model, rubric_version).
--   last_attempt_at      when the attempt began, captured before the digest is
--                        read so an operator turn landing mid-call still compares
--                        as newer than the attempt.
--   attempt_count        every attempt, successful or not.
--   consecutive_failures attempts in a row that ended without the judge producing
--                        what it owed (LLM error, breaker trip, verdict insert
--                        failure); reset to 0 by the next clean attempt. Drives
--                        the backoff.
--
-- The primary key leads with session_id, so it also serves as the index for the
-- session_id foreign key.

CREATE TABLE IF NOT EXISTS session_judges.attempts (
    session_id           bigint      NOT NULL
                                     REFERENCES sessions.sessions(id) ON DELETE CASCADE,
    judge_model          text        NOT NULL,
    rubric_version       integer     NOT NULL CHECK (rubric_version >= 1),
    last_attempt_at      timestamptz NOT NULL,
    attempt_count        integer     NOT NULL DEFAULT 1 CHECK (attempt_count >= 1),
    consecutive_failures integer     NOT NULL DEFAULT 0 CHECK (consecutive_failures >= 0),
    PRIMARY KEY (session_id, judge_model, rubric_version)
);

COMMENT ON TABLE session_judges.attempts IS
    'One row per (session, judge_model, rubric_version) the session judge has attempted, whether or '
    'not the attempt produced a verdict. Read by fetch_sessions_needing_judgment so a session is '
    'not re-selected when nothing has changed since its last attempt.';

-- Seed from history, set-based and idempotent, touching no existing row.
-- A session whose latest verdict at a model + rubric is `unknown` was, by
-- construction, attempted and yielded no overview (the judge writes an overview
-- only for a known outcome). Seeding it at the time of that verdict means it is
-- re-selected only if an operator turn arrives later, which the existing
-- unknown-verdict rule already covers. Sessions with a known verdict are
-- deliberately not seeded: a genuine overview backfill is still owed to them
-- once, after which the live attempt record stops it recurring.
WITH latest AS (
    SELECT DISTINCT ON (v.session_id, v.judge_model, v.rubric_version)
           v.session_id,
           v.judge_model,
           v.rubric_version,
           v.judged_at,
           o.code AS outcome_code
      FROM session_judges.verdicts v
      JOIN session_judges.outcomes o ON o.id = v.outcome_id
     ORDER BY v.session_id, v.judge_model, v.rubric_version, v.judged_at DESC, v.id DESC
),
counts AS (
    SELECT v.session_id,
           v.judge_model,
           v.rubric_version,
           count(*)::integer AS verdict_count
      FROM session_judges.verdicts v
     GROUP BY v.session_id, v.judge_model, v.rubric_version
)
INSERT INTO session_judges.attempts
       (session_id, judge_model, rubric_version, last_attempt_at, attempt_count)
SELECT l.session_id,
       l.judge_model,
       l.rubric_version,
       l.judged_at,
       c.verdict_count
  FROM latest l
  JOIN counts c
    ON c.session_id = l.session_id
   AND c.judge_model = l.judge_model
   AND c.rubric_version = l.rubric_version
 WHERE l.outcome_code = 'unknown'
ON CONFLICT (session_id, judge_model, rubric_version) DO NOTHING;
