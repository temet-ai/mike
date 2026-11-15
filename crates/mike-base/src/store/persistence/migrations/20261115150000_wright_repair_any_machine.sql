-- Wright's repair worker runs on every machine with a mike checkout, not under one machine's
-- lease, so the database keeps two machines from repairing the same problem at once: at most
-- one live repair per problem. The claim skips a finding that would break it.
CREATE UNIQUE INDEX IF NOT EXISTS wright_repairs_one_live_per_problem
    ON wright.repairs (problem_id)
    WHERE state IN ('editing', 'prepared', 'gating', 'landed', 'deployed');
