SELECT http_status, created_at
   FROM llm.calls
  WHERE created_at > now() - interval '24 hours'
  ORDER BY created_at DESC
  LIMIT 20