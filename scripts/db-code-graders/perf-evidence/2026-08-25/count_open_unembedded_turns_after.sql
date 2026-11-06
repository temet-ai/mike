SELECT COUNT(*)::bigint
  FROM memory.operator_turns
 WHERE closed_at IS NULL
