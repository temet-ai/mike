-- session_judges.attempts: a rolling 24-hour attempt window per (session, judge_model, rubric_version).
--
-- Why. Between 2026-09-02 and 2026-09-21 the session judge made 34,824 successful LLM calls for
-- 515 verdict rows: fetch_sessions_needing_judgment re-selected settled sessions whose overview was
-- NULL, and the writer wrote neither a verdict row nor an overview for them, so the same sessions
-- were judged on every 120-second poll (commit a4f40c45; JC13 §3.4). a4f40c45 added this table and
-- made every call record an attempt, which stopped that loop. It left no ceiling: a session whose
-- attempts keep failing is retried with backoff for ever, and any future call path that forgets to
-- record an attempt loops again. The window below lets every selection refuse a session that has
-- already had session_judge.max_attempts_per_day attempts (default 3) in the last 24 hours, so a
-- selection defect costs at most that many calls per session per day. Since the fix the most any
-- session has used is 2 in one day (926 calls over 916 session-days, 2026-09-21 13:10Z to 09-23).
--
-- record_judge_attempt maintains the window in its existing upsert: when window_started_at is NULL
-- or more than 24 hours before the attempt, the window restarts (window_started_at = the attempt
-- time, window_attempts = 1); otherwise window_attempts is incremented. Existing rows start with a
-- NULL window, which every selection treats as "under the cap".
--
-- Expand-only: two nullable-or-defaulted columns and one CHECK. Adding a column with a constant
-- default is catalogue-only on PostgreSQL 11 and later; the CHECK validates 2,524 rows (2026-09-24).

ALTER TABLE session_judges.attempts
    ADD COLUMN IF NOT EXISTS window_started_at timestamptz NULL,
    ADD COLUMN IF NOT EXISTS window_attempts   integer     NOT NULL DEFAULT 0;

ALTER TABLE session_judges.attempts
    DROP CONSTRAINT IF EXISTS attempts_window_attempts_check;

ALTER TABLE session_judges.attempts
    ADD CONSTRAINT attempts_window_attempts_check CHECK (window_attempts >= 0);

COMMENT ON COLUMN session_judges.attempts.window_started_at IS
    'Start of the current rolling 24-hour attempt window for this (session, judge_model, '
    'rubric_version). NULL until the first attempt after this column was added. Maintained by '
    'record_judge_attempt; read by every session-judge selection together with window_attempts.';

COMMENT ON COLUMN session_judges.attempts.window_attempts IS
    'Attempts (LLM calls, successful or not) inside the current window. A selection skips the '
    'session while window_attempts >= session_judge.max_attempts_per_day and the window is younger '
    'than 24 hours. Shadow attempts are recorded under judge_model = ''shadow:'' || model.';
