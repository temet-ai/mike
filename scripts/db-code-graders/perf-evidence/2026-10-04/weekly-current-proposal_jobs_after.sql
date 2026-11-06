SELECT status,count(*) AS jobs,
          min(created_at) AS oldest_created_at,min(updated_at) AS oldest_updated_at,
          min(next_attempt_at) FILTER(WHERE status='awaiting_embedding') AS next_attempt_at,
          count(*) FILTER(WHERE status='awaiting_embedding' AND next_attempt_at <= now()) AS retry_ready,
          max(retry_count) AS max_retries
          FROM memory.lesson_proposal_jobs GROUP BY status ORDER BY status;
