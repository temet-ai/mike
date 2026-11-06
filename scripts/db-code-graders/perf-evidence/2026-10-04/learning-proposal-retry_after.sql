UPDATE memory.lesson_proposal_jobs SET updated_at = NOW(),
                retry_count = least(retry_count + 1, 16),
                next_attempt_at = NOW() + make_interval(secs => least(3600, 15 * power(2, least(retry_count + 1, 12))::integer))
             WHERE verdict_id = 4