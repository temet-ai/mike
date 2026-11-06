SELECT recovery_state,count(*) AS jobs,
          min(created_at) AS oldest_created_at
          FROM memory.lesson_proposal_recovery GROUP BY recovery_state ORDER BY recovery_state;
