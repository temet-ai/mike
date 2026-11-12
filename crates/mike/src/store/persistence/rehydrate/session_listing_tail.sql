         SELECT ranked.session_id,
                ranked.project_path,
                ranked.project_id,
                ranked.cwd,
                ranked.project_resolution,
                ranked.first_seen,
                ranked.last_seen,
                rc.request_count,
                COALESCE(observation.last_status, newest_status.live_status, ranked.last_status) AS last_status,
                array_remove(ARRAY[observation.model], NULL) AS last_models,
                ranked.title,
                ranked.pinned,
                ranked.project_pinned,
                NULL::float8 AS avg_latency_ms,
                COALESCE(first_op.user_prompt, first_text.human_prompt) AS originating_operator_prompt,
                sjq.session_outcome,
                sjq.session_quality_score,
                sjq.session_judged_at,
                ranked.overview,
                ranked.overview_generated_at,
                COALESCE(mbx.pending_inbox_count, 0) AS pending_inbox_count,
         pt.projects_touched
         FROM ranked
         LEFT JOIN runtime.session_observation_v observation ON observation.session_id = ranked.id
         LEFT JOIN LATERAL (
             -- Per-session request count + distinct project_id set (issue #32),
             -- one scan of requests.requests via idx_requests_session_id
             -- shared between both (see list_sessions_paginated's doc comment).
             SELECT COUNT(r.id)::int4 AS request_count,
                    ARRAY_AGG(DISTINCT r.project_id) AS distinct_project_ids
             FROM runtime.model_call_facts r
             WHERE r.session_id = ranked.id
         ) rc ON TRUE
         LEFT JOIN LATERAL (
             -- Newest request's live terminal status (issue #147) --
             -- Index Scan Backward on idx_requests_session_created_id,
             -- LIMIT 1. See fetch_session_head's #142 fix for the
             -- cache-staleness rationale. Permitted LATERAL per
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
              WHERE nr.session_id = ranked.id
              ORDER BY nr.created_at DESC, nr.id DESC
              LIMIT 1
         ) newest_status ON TRUE
         LEFT JOIN LATERAL (
             -- Drive from classifications (session_id, kind_id='operator') so
             -- idx_pk_classifications_session_kind enables seek+early-stop on
             -- request_id ASC. kind_id resolved via uncorrelated subquery (runs
             -- once). Mirrors the ae88b347 session_head fix.
             SELECT COALESCE(ot.user_prompt, sr.human_prompt) AS user_prompt
               FROM (
                   SELECT pk_cls.request_id
                     FROM prompt_kinds.classifications pk_cls
                    WHERE pk_cls.session_id = ranked.id
                      AND pk_cls.kind_id = (
                          SELECT id FROM prompt_kinds.kinds WHERE code = 'operator'
                      )
                    ORDER BY pk_cls.request_id ASC
                    LIMIT 1
               ) first_cls
               JOIN memory.operator_turns ot
                 ON ot.start_request_id = first_cls.request_id
               LEFT JOIN requests.requests sr ON sr.id = first_cls.request_id
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
                AND r_any.session_id = ranked.id
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
             -- Latest session-judge verdict (top-1-per-group, bounded to the
             -- ≤50 page). Permitted LATERAL per .claude/rules/sql-set-based.md.
             SELECT o.code AS session_outcome,
                    v.quality_score::float8 AS session_quality_score,
                    v.judged_at AS session_judged_at
               FROM session_judges.verdicts v
               JOIN session_judges.outcomes o ON o.id = v.outcome_id
              WHERE v.session_id = ranked.id
                AND v.mode = 'live'
              ORDER BY v.judged_at DESC, v.id DESC
              LIMIT 1
         ) sjq ON TRUE
         LEFT JOIN (
             -- Undelivered mailbox count, aggregated once for the whole
             -- ≤50-row page rather than per-row LATERAL: mailbox.messages
             -- is small today, so a per-row LATERAL degrades to a full
             -- table scan repeated per session (the planner won't pick
             -- idx_mailbox_messages_pending until the table is much
             -- bigger than the page it's being probed against). Grouping
             -- is scoped to `sent_request_id IS NULL` — the partial
             -- index's predicate — so this scan stays bounded by the
             -- (operationally small) undelivered-mail set regardless of
             -- how large mailbox.messages grows overall; no separate
             -- page-id filter is needed since the outer join already
             -- discards groups outside this page. A session with zero
             -- undelivered mail has no row here at all (GROUP BY only
             -- emits rows that exist), so the outer SELECT must
             -- COALESCE the join miss to 0 — `pending_inbox_count` is
             -- NOT NULL on the wire.
             SELECT m.to_session_id, COUNT(*)::int4 AS pending_inbox_count
               FROM mailbox.messages m
              WHERE m.sent_request_id IS NULL
              GROUP BY m.to_session_id
         ) mbx ON mbx.to_session_id = ranked.id
         LEFT JOIN LATERAL (
             -- Distinct project paths this session's own requests touched
             -- (issue #32 -- requests.requests.project_id can diverge from
             -- the session's launch-time project_id: nested-repo cd /
             -- subagent work under a parent workspace cwd). Bounded to
             -- idx_requests_session_id, same cost class as the rc LATERAL
             -- above. Permitted LATERAL per .claude/rules/sql-set-based.md.
             SELECT COALESCE(
                        ARRAY_AGG(pp.project_path ORDER BY pp.project_path),
                        ARRAY[]::text[]
                    ) AS projects_touched
               FROM projects.projects pp
              WHERE pp.id = ANY(rc.distinct_project_ids)
         ) pt ON TRUE
