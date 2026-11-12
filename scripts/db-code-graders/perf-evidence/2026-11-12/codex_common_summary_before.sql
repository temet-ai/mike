SELECT s.session_uuid::text AS "session_id!",
        p.project_path AS "project_path!",
        CASE WHEN s.project_resolution = 'unresolved' OR p.project_path = '(no project)' THEN NULL ELSE p.id END AS "project_id?",
        s.cwd AS "cwd?",
        s.project_resolution AS "project_resolution!",
        s.first_seen AS "first_seen!",
        s.last_seen AS "last_seen!",
        COALESCE(MAX(newest_status.live_status), s.last_status) AS "last_status?",
        s.title AS "title?",
        s.pinned AS "pinned!",
        p.pinned AS "project_pinned!",
        COUNT(r.id)::int4 AS "request_count!",
        COALESCE(
            ARRAY_AGG(DISTINCT r.model ORDER BY r.model)
                FILTER (WHERE r.model IS NOT NULL),
            ARRAY[]::text[]
        ) AS "last_models!",
        AVG(EXTRACT(EPOCH FROM (r.completed_at - r.created_at)) * 1000.0)
            FILTER (WHERE r.completed_at IS NOT NULL)::float8 AS "avg_latency_ms?",
        COALESCE(first_op.user_prompt, first_text.human_prompt) AS "originating_operator_prompt?",
        sjq.session_outcome AS "session_outcome?",
        sjq.session_quality_score AS "session_quality_score?",
        sjq.session_judged_at AS "session_judged_at?",
        s.overview AS "overview?",
        s.overview_generated_at AS "overview_generated_at?",
        mbx.pending_inbox_count AS "pending_inbox_count!",
        -- Distinct project paths this session's own requests touched
        -- (issue #32 — requests.requests.project_id can diverge from
        -- the session's launch-time project_id). `r` is already
        -- joined for request_count/last_models, so this rides the
        -- same GROUP BY aggregate rather than a second LATERAL.
        COALESCE(
            ARRAY_AGG(DISTINCT rp.project_path ORDER BY rp.project_path)
                FILTER (WHERE rp.project_path IS NOT NULL),
            ARRAY[]::text[]
        ) AS "projects_touched!"
 FROM sessions.sessions s
 JOIN projects.projects p ON p.id = s.project_id
 LEFT JOIN requests.requests r ON r.session_id = s.id
 LEFT JOIN projects.projects rp ON rp.id = r.project_id
 LEFT JOIN LATERAL (
     SELECT COALESCE(ot.user_prompt, sr.human_prompt) AS user_prompt
       FROM prompt_kinds.classifications pk_cls
       JOIN prompt_kinds.kinds pk_kinds
         ON pk_kinds.id = pk_cls.kind_id AND pk_kinds.code = 'operator'
       JOIN memory.operator_turns ot
         ON ot.start_request_id = pk_cls.request_id
       LEFT JOIN requests.requests sr ON sr.id = ot.start_request_id
      WHERE pk_cls.session_id = s.id
      ORDER BY pk_cls.request_id ASC
      LIMIT 1
 ) first_op ON TRUE
 LEFT JOIN LATERAL (
     -- Fallback name for a session the classification above cannot reach
     -- (issue #31). Branch 1 needs BOTH a prompt_kinds 'operator'
     -- classification and a memory.operator_turns row; a dispatched
     -- subagent, a spawned peer and a hook-driven session receive no
     -- operator prompt to classify, so 47 of the 77 title-less sessions
     -- with real work in them resolved to NULL and rendered as a hex id.
     --
     -- This is the session's OWN earliest captured prompt on a lane that
     -- carries authored words — not a guess and not a generated title,
     -- but its first words, which is what the title sidecar would have
     -- summarised had the harness run one. Sidecar and probe origins are
     -- excluded: their text is machinery, not the session's subject.
     --
     -- `first_op.user_prompt IS NULL` is load-bearing, not tidiness. It
     -- keeps this scan off every session branch 1 already answered
     -- (25 of 50 on a live page); without it the same page measured
     -- 5.993 ms against 1.326 ms. Ordering on (created_at, id) matches
     -- idx_requests_session_created_id so the scan stops at the first
     -- match — ~8 rows — instead of reading the session and sorting, and
     -- the origin codes resolve through an uncorrelated subquery rather
     -- than a join that would defeat that index order.
     --
     -- The fallback fires ONLY for a row the classifier never
     -- touched (issue #180). Without the `NOT EXISTS` below, a
     -- turn classified as something other than 'operator' (e.g.
     -- 'activity-label') still fell through to its own
     -- human_prompt, because this scan carried no prompt_kind
     -- filter of its own -- it just wanted ANY early
     -- human-authored text. That defeated branch 1's contract: a
     -- session whose only turns are classified non-operator must
     -- report NULL here, not that turn's own text.
     SELECT r_any.human_prompt
       FROM requests.requests r_any
      WHERE first_op.user_prompt IS NULL
        AND r_any.session_id = s.id
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
 LEFT JOIN LATERAL (
     -- Latest session-judge verdict (top-1-per-group, bounded to this
     -- one session). Permitted LATERAL per .claude/rules/sql-set-based.md.
     SELECT o.code AS session_outcome,
            v.quality_score::float8 AS session_quality_score,
            v.judged_at AS session_judged_at
       FROM session_judges.verdicts v
       JOIN session_judges.outcomes o ON o.id = v.outcome_id
      WHERE v.session_id = s.id
        AND v.mode = 'live'
      ORDER BY v.judged_at DESC, v.id DESC
      LIMIT 1
 ) sjq ON TRUE
 LEFT JOIN LATERAL (
     -- Undelivered mailbox count for this session (queued or
     -- claimed-but-unsent — a delivered row has sent_request_id
     -- set). Bounded via idx_mailbox_messages_pending
     -- (to_session_id, id) WHERE sent_request_id IS NULL —
     -- single index scan, one row per session. Permitted LATERAL
     -- per .claude/rules/sql-set-based.md.
     SELECT COUNT(*)::int4 AS pending_inbox_count
       FROM mailbox.messages m
      WHERE m.to_session_id = s.id
        AND m.sent_request_id IS NULL
 ) mbx ON TRUE
 LEFT JOIN LATERAL (
     -- Newest request's own terminal status, live -- the cached
     -- sessions.sessions.last_status column can go stale when a
     -- request's in-flight guard tears down without reaching a
     -- terminal write (issue #147, same cache-staleness root
     -- cause as fetch_session_head's #142 fix below). Index Scan
     -- Backward on idx_requests_session_created_id, LIMIT 1 --
     -- cheap even for large sessions. Permitted LATERAL per
     -- .claude/rules/sql-set-based.md.
     SELECT CASE st.code
                WHEN 'pending'   THEN 'Pending'
                WHEN 'streaming' THEN 'Streaming'
                WHEN 'complete'  THEN 'Complete'
                WHEN 'error'     THEN 'Error: ' || COALESCE(re.message, '')
            END AS live_status
       FROM requests.requests nr
       JOIN requests.statuses st ON st.id = nr.status_id
       LEFT JOIN requests.errors re ON re.request_id = nr.id
      WHERE nr.session_id = s.id
      ORDER BY nr.created_at DESC, nr.id DESC
      LIMIT 1
 ) newest_status ON TRUE
 WHERE s.id = 1
 GROUP BY s.id, p.id, first_op.user_prompt, first_text.human_prompt,
          sjq.session_outcome, sjq.session_quality_score, sjq.session_judged_at,
          mbx.pending_inbox_count
