WITH origins AS (
     SELECT r.id, r.session_id, r.agent_instance_id, r.created_at,
            pto.code AS turn_origin_code
       FROM requests.requests r
       JOIN requests.turn_origins pto ON pto.id = r.turn_origin_id
 ),
 flagged AS (
     SELECT id, session_id, created_at,
            COALESCE(agent_instance_id, 0) AS part_key,
            ((agent_instance_id IS NULL AND turn_origin_code IN ('primary', 'primary_first'))
          OR (agent_instance_id IS NOT NULL AND turn_origin_code = 'subagent')) AS is_candidate
       FROM origins
 ),
 counted AS (
     SELECT id, session_id, part_key, created_at, is_candidate,
            SUM(CASE WHEN is_candidate THEN 1 ELSE 0 END) OVER (
                PARTITION BY session_id, part_key ORDER BY created_at, id
                ROWS BETWEEN UNBOUNDED PRECEDING AND 1 PRECEDING
            ) AS prior_candidate_count
       FROM flagged
 ),
 candidates_ordinal AS (
     SELECT id, session_id, part_key,
            ROW_NUMBER() OVER (
                PARTITION BY session_id, part_key ORDER BY created_at, id
            ) AS ordinal
       FROM flagged
      WHERE is_candidate
 )
 UPDATE requests.requests r
    SET predecessor_request_id = co.id
   FROM counted c
   LEFT JOIN candidates_ordinal co
          ON co.session_id = c.session_id
         AND co.part_key   = c.part_key
         AND co.ordinal    = c.prior_candidate_count
  WHERE c.id = r.id
    AND EXISTS (SELECT 1 FROM requests.messages m WHERE m.request_id = r.id)
    AND r.predecessor_request_id IS DISTINCT FROM co.id;
