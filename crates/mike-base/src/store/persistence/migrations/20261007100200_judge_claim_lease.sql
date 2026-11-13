-- The turn judge claims a turn with a lease (issue #381).
--
-- The three claim paths (live sweep, immediate per-session, re-judge run)
-- were plain SELECTs, and judge_next_attempt_at is written only after a
-- failure, so two workers running together could claim and judge the same
-- turn: 89 turns got two live verdicts on 2026-09-28.
--
-- judge_claimed_until is the lease. A claim sets it to now() + the lease
-- length in the same statement that selects the turn (FOR UPDATE SKIP LOCKED),
-- and every claim query skips a turn whose lease has not run out. The worker
-- clears it when it has finished with the turn; if the worker dies, the lease
-- runs out on its own. It is a separate column from judge_next_attempt_at
-- because that one is the failure backoff, and clearing a lease must never
-- erase a backoff.
--
-- Nullable by design: NULL means no worker holds the turn, which is every
-- turn at rest.

ALTER TABLE memory.operator_turns
    ADD COLUMN IF NOT EXISTS judge_claimed_until timestamptz;

COMMENT ON COLUMN memory.operator_turns.judge_claimed_until IS
    'Lease on a turn a judge worker has claimed: no other claim takes the '
    'turn while this is in the future. Set by the claim, cleared when the '
    'worker has finished, expires by itself if the worker dies. NULL when '
    'no worker holds the turn. Separate from judge_next_attempt_at, which '
    'is the failure backoff.';
