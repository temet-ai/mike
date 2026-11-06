SELECT created_at::date AS day,count(*) AS calls,sum(prompt_tokens) AS input_tokens,
          sum(completion_tokens) AS output_tokens FROM llm.calls WHERE role='lesson_proposer'
          AND created_at >= '2026-09-27T17:40:00Z' AND created_at < '2026-10-04T17:40:00Z' GROUP BY 1 ORDER BY 1