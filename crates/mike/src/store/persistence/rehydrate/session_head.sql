SELECT
     s.session_uuid::text AS "session_id!",
     COALESCE(newest.live_status, s.last_status) AS "last_status?",
     newest.id            AS "newest_request_id?",
     newest.rn            AS "newest_request_number?",
     COALESCE(first_op.user_prompt, first_text.human_prompt) AS "originating_operator_prompt?"
 FROM sessions.sessions s
 LEFT JOIN LATERAL (
     SELECT r.id,
            ROW_NUMBER() OVER (
                PARTITION BY r.session_id
                ORDER BY r.created_at ASC, r.id ASC
            ) AS rn,
            -- Mirrors recover_stale_pending_rows' status-to-string mapping —
            -- keep the two in sync.
            CASE st.code
                WHEN 'pending'   THEN 'Pending'
                WHEN 'streaming' THEN 'Streaming'
                WHEN 'complete'  THEN 'Complete'
                WHEN 'error'     THEN 'Error: ' || COALESCE(re.message, '')
            END AS live_status
     FROM requests.requests r
     JOIN requests.statuses st ON st.id = r.status_id
     LEFT JOIN requests.errors re ON re.request_id = r.id
     WHERE r.session_id = $1
     ORDER BY r.created_at DESC, r.id DESC
     LIMIT 1
 ) newest ON TRUE
 LEFT JOIN LATERAL (
     -- Drive from classifications scoped to (session_id, kind_id='operator')
     -- so the planner can use idx_pk_classifications_session_kind and stop
     -- early on the first matching request_id ASC. The kind_id is resolved
     -- via an uncorrelated subquery (runs once, not per outer row).
     SELECT COALESCE(ot.user_prompt, sr.human_prompt) AS user_prompt
       FROM (
           SELECT pk_cls.request_id
             FROM prompt_kinds.classifications pk_cls
            WHERE pk_cls.session_id = $1
              AND pk_cls.kind_id = (SELECT id FROM prompt_kinds.kinds WHERE code = 'operator')
            ORDER BY pk_cls.request_id ASC
            LIMIT 1
       ) first_cls
       JOIN memory.operator_turns ot ON ot.start_request_id = first_cls.request_id
       LEFT JOIN requests.requests sr ON sr.id = first_cls.request_id
 ) first_op ON TRUE
 LEFT JOIN LATERAL (
     -- Fallback title for a session branch 1 cannot reach: a dispatched
     -- subagent, a spawned peer, or a hook-driven session gets no operator
     -- prompt to classify. Uses the session's own earliest human-authored
     -- prompt, excluding sidecar and probe origins.
     --
     -- `first_op.user_prompt IS NULL` is a required performance guard, not
     -- cosmetic — it keeps this scan off every session branch 1 already
     -- answered. Ordering on (created_at, id) matches
     -- idx_requests_session_created_id so the scan stops at the first
     -- match instead of reading the whole session and sorting.
     --
     -- The `NOT EXISTS` fires the fallback only for a row the classifier
     -- never touched. Without it, a turn classified as something other
     -- than 'operator' would still fall through to its own human_prompt,
     -- when the contract requires NULL here instead.
     SELECT r_any.human_prompt
       FROM requests.requests r_any
      WHERE first_op.user_prompt IS NULL
        AND r_any.session_id = $1
        AND r_any.human_prompt IS NOT NULL
        AND r_any.human_prompt <> ''
        AND NOT EXISTS (
            SELECT 1 FROM prompt_kinds.classifications pk_any
             WHERE pk_any.request_id = r_any.id
        )
        AND r_any.turn_origin_id IN (
            SELECT id FROM requests.turn_origins
             WHERE code IN (
                 'primary', 'primary_first', 'subagent', 'workflow_subagent'
             )
        )
      ORDER BY r_any.created_at ASC, r_any.id ASC
      LIMIT 1
 ) first_text ON TRUE
 WHERE s.id = $1
