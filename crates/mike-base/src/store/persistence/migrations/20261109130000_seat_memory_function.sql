-- memory.seat_memory(p_project_id): the allow-listed seat tier of memory-keeper search (P7).
--
-- Machine-opened operator_turns are excluded from every operator-facing reader
-- (opened_by IS NULL OR 'operator'). Seat work is still where most "find the
-- earlier work" answers live, so it enters search through this function only: one
-- row per included item, labelled with author, run, seat and task, and framed
-- by the reader as data, never as something the operator said.
--
-- A function rather than a view because every read is for one project. The seat set is
-- narrowed to the project before any turn, mail or note is touched; a view over all projects
-- cannot push a project filter below its window functions and took about 530 ms per read
-- on the capture data (budget 50 ms).
--
-- What is included (item_kind):
--   seat_brief         launch.launches.brief_text of a seat, once per (run, seat, text)
--   workflow_brief     brief of a spawned worker that belongs to no fleet run, once per launch
--   seat_mail          coordinator <-> seat mail (mailbox.messages body, delivered as a channel turn)
--   peer_message       seat <-> seat mail, and teammate-message turns (subagent hand-backs)
--   seat_report        final_assistant_text of fleet_seat turns, and the report note a seat filed
--   seat_thread_entry  work.thread_log entries written by a seat session
--   seat_goal_feedback Stop-hook feedback, once per (run, seat, task)
--
-- Gates:
--   * the seat's run has a launch.fleet_events 'report_filed' event;
--   * the launch did not fail; a template seat counts only when it filed its own report;
--   * observer seats and synthetic sessions are out.
--
-- Left out because they are not memory: empty prompts, task-notification, loop-wakeup,
-- tool-loaded, analysis-probe, compaction, observer and duty polling. Mail and
-- teammate-message rows collapse per receiving session on the first 120 characters of
-- their text (read from the first 300) with <channel> and <agent-message> tags, UUIDs and
-- digits removed and whitespace runs collapsed (the repeat_key recipe of
-- prompt_kinds.classifications, applied after the envelope is stripped, because the
-- stored repeat_key of a channel turn is made of its attributes); the first is kept
-- and repeat_count says how many collapsed into it. Stop-hook feedback collapses per
-- (run, seat, task). Blank means the first 64 characters (300 for mail) are whitespace;
-- text is read with substr so a long body is not decompressed to test for it.
--
-- Mail rows carry from_session (the sender's session_uuid, from mailbox.messages.from_session_id)
-- and from_role (the role attribute of the channel tag where it has one, otherwise the
-- seat the sender was launched as). Both are NULL on every other kind.
--
-- verdict_accepted: every criterion's latest 'verdict_recorded' for (run, task) passed;
-- false when the item has no task or no verdict. report_filed: the item's own seat
-- session filed a report (the run-level gate is applied to every row).
--
-- Payload values are compared as jsonb and uuid text is cast only after a pattern check,
-- so a malformed fleet_events payload yields a non-match instead of an error.
--
-- Read-only; nothing is written. It relies on idx_fleet_events_session_kind and
-- idx_work_thread_log_session_uuid, created by the two migrations that follow.
CREATE OR REPLACE FUNCTION memory.seat_memory(p_project_id bigint)
RETURNS TABLE (
    item_kind text,
    project_id bigint,
    session_id bigint,
    session_uuid uuid,
    run_id text,
    seat text,
    task text,
    author text,
    from_session uuid,
    from_role text,
    at timestamptz,
    text text,
    source_ref text,
    repeat_count integer,
    report_filed boolean,
    verdict_accepted boolean
)
LANGUAGE sql
STABLE
PARALLEL SAFE
AS $function$
WITH
seats AS (
    SELECT DISTINCT ON (l.session_uuid)
           l.id AS launch_id, l.session_uuid, l.run_id, l.seat, l.brief_text,
           COALESCE(l.opened_at, l.created_at) AS launched_at,
           s.id AS session_id, s.project_id,
           st.task,
           (rf.hit IS NOT NULL) AS report_filed
    FROM sessions.sessions s
    JOIN launch.launches l ON l.session_uuid = s.session_uuid
    LEFT JOIN LATERAL (
        SELECT 1 AS hit
        FROM launch.fleet_events e
        WHERE e.session_uuid = l.session_uuid AND e.kind = 'report_filed'
        LIMIT 1
    ) rf ON true
    LEFT JOIN (SELECT DISTINCT t.session_uuid FROM launch.templates t) tm
           ON tm.session_uuid = l.session_uuid
    LEFT JOIN LATERAL (
        SELECT e.task
        FROM launch.fleet_events e
        WHERE e.session_uuid = l.session_uuid AND e.task IS NOT NULL
        ORDER BY e.id
        LIMIT 1
    ) st ON true
    WHERE s.project_id = p_project_id
      AND l.result <> 'failed'
      AND EXISTS (SELECT 1 FROM launch.fleet_events g
                  WHERE g.run_id = l.run_id AND g.kind = 'report_filed')
      AND NOT EXISTS (SELECT 1 FROM memory.turn_facts tf
                      WHERE tf.session_id = s.id AND tf.session_class = 'synthetic')
      AND COALESCE(l.seat, '') !~ '^observer'
      AND l.agent IS DISTINCT FROM 'fleet-observer'
      AND (rf.hit IS NOT NULL
           OR (tm.session_uuid IS NULL AND COALESCE(l.seat, '') !~ '^template-'))
    ORDER BY l.session_uuid, l.id DESC
),
verdicts AS (
    SELECT v.run_id, v.task, bool_and(v.passed) AS accepted
    FROM (
        SELECT DISTINCT ON (e.run_id, e.task, e.payload ->> 'criterion')
               e.run_id, e.task, (e.payload -> 'passed' = 'true'::jsonb) AS passed
        FROM launch.fleet_events e
        WHERE e.kind = 'verdict_recorded'
          AND e.run_id IN (SELECT DISTINCT s.run_id FROM seats s)
        ORDER BY e.run_id, e.task, e.payload ->> 'criterion', e.id DESC
    ) v
    GROUP BY v.run_id, v.task
),
seat_turns AS MATERIALIZED (
    SELECT s.session_id, s.session_uuid, s.run_id, s.seat, s.task, s.report_filed, s.project_id,
           ot.id AS turn_id, ot.opened_by, ot.created_at, ot.closed_at, k.code AS kind_code
    FROM seats s
    JOIN memory.operator_turns ot ON ot.session_id = s.session_id
    LEFT JOIN prompt_kinds.classifications c ON c.request_id = ot.start_request_id
    LEFT JOIN prompt_kinds.kinds k ON k.id = c.kind_id
),
mail_turns AS (
    SELECT st.session_id, st.session_uuid, st.run_id, st.seat, st.task, st.report_filed, st.project_id,
           m.tag[1] AS tag
    FROM seat_turns st
    JOIN memory.operator_turns ot ON ot.id = st.turn_id
    CROSS JOIN LATERAL regexp_matches(ot.user_prompt, '<channel ([^>]*)>', 'g') AS m(tag)
    WHERE st.opened_by = 'machine'
      AND st.kind_code IN ('harness-authored', 'mailbox-delivery')
      AND substr(ot.user_prompt, 1, 9) = '<channel '
),
mail AS (
    SELECT mt.session_id, mt.session_uuid, mt.run_id, mt.seat, mt.task, mt.report_filed, mt.project_id,
           mm.message_uuid, mm.queued_at, mm.body, substr(mm.body, 1, 300) AS head,
           fs.session_uuid AS from_session,
           COALESCE(NULLIF(substring(mt.tag FROM 'from_role="([^"]*)"'), ''), fl.seat, '') AS from_role
    FROM mail_turns mt
    JOIN mailbox.messages mm
         ON mm.message_uuid = CASE
                WHEN substring(mt.tag FROM 'message_id="([^"]*)"')
                     ~ '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                THEN substring(mt.tag FROM 'message_id="([^"]*)"')::uuid
            END
    LEFT JOIN sessions.sessions fs ON fs.id = mm.from_session_id
    LEFT JOIN LATERAL (
        SELECT fl2.seat
        FROM launch.launches fl2
        WHERE fl2.session_uuid = fs.session_uuid AND fl2.seat IS NOT NULL
        ORDER BY fl2.id DESC
        LIMIT 1
    ) fl ON true
),
solo AS (
    SELECT * FROM (
        SELECT DISTINCT ON (s.run_id, s.seat, s.brief_text)
               'seat_brief'::text AS item_kind, s.project_id, s.session_id, s.session_uuid,
               s.run_id, s.seat, s.task, 'coordinator'::text AS author,
               NULL::uuid AS from_session, NULL::text AS from_role,
               s.launched_at AS at, s.brief_text AS text,
               'launch:' || s.launch_id AS source_ref,
               s.report_filed
        FROM seats s
        WHERE btrim(s.brief_text) <> ''
        ORDER BY s.run_id, s.seat, s.brief_text, s.launch_id
    ) b

    UNION ALL
    SELECT 'workflow_brief', se.project_id, se.id, l.session_uuid,
           NULL, NULL, NULL, 'coordinator',
           NULL, NULL,
           COALESCE(l.opened_at, l.created_at), l.brief_text,
           'launch:' || l.id, false
    FROM sessions.sessions se
    JOIN launch.launches l ON l.session_uuid = se.session_uuid
    WHERE se.project_id = p_project_id
      AND l.run_id IS NULL
      AND l.result = 'success'
      AND btrim(l.brief_text) <> ''
      AND EXISTS (SELECT 1 FROM memory.turn_facts tf
                  WHERE tf.session_id = se.id AND tf.session_class = 'fleet_seat')
      AND NOT EXISTS (SELECT 1 FROM memory.turn_facts tf
                      WHERE tf.session_id = se.id AND tf.session_class = 'synthetic')

    UNION ALL
    SELECT 'seat_report', st.project_id, st.session_id, st.session_uuid,
           st.run_id, st.seat, st.task, 'seat',
           NULL, NULL,
           COALESCE(st.closed_at, st.created_at), ot.final_assistant_text,
           'turn:' || st.turn_id, st.report_filed
    FROM seat_turns st
    JOIN memory.turn_facts tf ON tf.operator_turn_id = st.turn_id AND tf.session_class = 'fleet_seat'
    JOIN memory.operator_turns ot ON ot.id = st.turn_id
    WHERE btrim(substr(ot.final_assistant_text, 1, 64)) <> ''
      AND (st.kind_code IS NULL OR st.kind_code <> ALL (ARRAY[
          'empty-prompt', 'task-notification', 'loop-wakeup', 'tool-loaded',
          'analysis-probe', 'compaction', 'channel-wake', 'activity-label',
          'background-event', 'session-recap', 'steering-payload']))

    UNION ALL
    (SELECT DISTINCT ON (n.note_uuid)
           'seat_report', s.project_id, s.session_id, s.session_uuid,
           s.run_id, s.seat, s.task, 'seat',
           NULL, NULL,
           n.created_at, n.body,
           'note:' || n.note_uuid, s.report_filed
    FROM seats s
    JOIN launch.fleet_events e ON e.session_uuid = s.session_uuid AND e.kind = 'report_filed'
    JOIN workspace.session_notes n
         ON n.note_uuid = CASE
                WHEN e.payload ->> 'note_uuid'
                     ~ '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                THEN (e.payload ->> 'note_uuid')::uuid
            END
    WHERE btrim(substr(n.body, 1, 64)) <> ''
    ORDER BY n.note_uuid, e.id)

    UNION ALL
    SELECT 'seat_thread_entry', th.project_id, s.session_id, s.session_uuid,
           s.run_id, s.seat, s.task, 'seat',
           NULL, NULL,
           tl.created_at, COALESCE(NULLIF(btrim(tl.body), ''), tl.summary),
           'entry:' || tl.id, s.report_filed
    FROM seats s
    JOIN work.thread_log tl ON tl.session_uuid = s.session_uuid
    JOIN work.threads th ON th.id = tl.thread_id
    WHERE btrim(substr(tl.body, 1, 64)) <> '' OR btrim(substr(tl.summary, 1, 64)) <> ''
),
collapsible (item_kind, project_id, session_id, session_uuid, run_id, seat, task, author,
             from_session, from_role, at, text, source_ref, report_filed, scope_key, head) AS (
    SELECT CASE WHEN ma.from_role !~ '^(coordinator|acceptance|observer|lead)' AND ma.from_role <> ''
                 AND ma.seat !~ '^(coordinator|acceptance)'
                THEN 'peer_message' ELSE 'seat_mail' END,
           ma.project_id, ma.session_id, ma.session_uuid, ma.run_id, ma.seat, ma.task,
           CASE WHEN ma.from_role ~ '^(coordinator|acceptance|lead)' OR ma.from_role = '' THEN 'coordinator'
                WHEN ma.seat ~ '^(coordinator|acceptance)' THEN 'seat'
                ELSE 'peer' END,
           ma.from_session, NULLIF(ma.from_role, ''),
           ma.queued_at, ma.body,
           'mail:' || ma.message_uuid, ma.report_filed, ma.session_id || ':' || 'mail', ma.head
    FROM mail ma
    WHERE ma.from_role !~ '^observer' AND btrim(ma.head) <> ''

    UNION ALL
    SELECT 'peer_message', st.project_id, st.session_id, st.session_uuid,
           st.run_id, st.seat, st.task,
           CASE WHEN ot.user_prompt LIKE '%[Subagent hand-back]%' THEN 'subagent' ELSE 'peer' END,
           NULL, NULL,
           st.created_at, ot.user_prompt,
           'turn:' || st.turn_id, st.report_filed, st.session_id || ':' || 'peer',
           substr(ot.user_prompt, 1, 300)
    FROM seat_turns st
    JOIN memory.operator_turns ot ON ot.id = st.turn_id
    WHERE st.opened_by = 'machine' AND st.kind_code = 'teammate-message'
      AND btrim(substr(ot.user_prompt, 1, 64)) <> ''

    UNION ALL
    SELECT 'seat_goal_feedback', st.project_id, st.session_id, st.session_uuid,
           st.run_id, st.seat, st.task, 'coordinator',
           NULL, NULL,
           st.created_at, ot.user_prompt,
           'turn:' || st.turn_id, st.report_filed,
           'feedback:' || st.run_id || '/' || COALESCE(st.seat, '') || '/' || COALESCE(st.task, ''),
           NULL
    FROM seat_turns st
    JOIN memory.operator_turns ot ON ot.id = st.turn_id
    WHERE st.opened_by = 'machine' AND st.kind_code = 'hook-feedback'
      AND btrim(substr(ot.user_prompt, 1, 64)) <> ''
),
keyed AS (
    SELECT c.*,
           CASE
               WHEN c.item_kind = 'seat_goal_feedback' THEN c.scope_key
               WHEN length(substr(c.head, 1, 40)) >= 40 THEN
                   c.scope_key || ':' || left(btrim(regexp_replace(regexp_replace(
                       lower(c.head),
                       '<(channel|agent-message)[^>]*>|[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}|[0-9]+',
                       '', 'g'), '\s{2,}|[\n\t\r\f\v]', ' ', 'g')), 120)
               ELSE c.scope_key || ':' || c.source_ref
           END AS collapse_key
    FROM collapsible c
),
collapsed AS (
    SELECT k.item_kind, k.project_id, k.session_id, k.session_uuid, k.run_id, k.seat, k.task,
           k.author, k.from_session, k.from_role, k.at, k.text, k.source_ref, k.report_filed,
           row_number() OVER w AS rn,
           count(*) OVER (w ROWS BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING) AS repeat_count
    FROM keyed k
    WINDOW w AS (PARTITION BY k.item_kind, k.collapse_key ORDER BY k.at, k.source_ref)
),
items AS (
    SELECT c.item_kind, c.project_id, c.session_id, c.session_uuid, c.run_id, c.seat, c.task,
           c.author, c.from_session, c.from_role, c.at, c.text, c.source_ref, c.report_filed,
           1 AS repeat_count
    FROM solo c

    UNION ALL
    SELECT r.item_kind, r.project_id, r.session_id, r.session_uuid, r.run_id, r.seat, r.task,
           r.author, r.from_session, r.from_role, r.at, r.text, r.source_ref, r.report_filed,
           r.repeat_count
    FROM collapsed r
    WHERE r.rn = 1
)
SELECT i.item_kind, i.project_id, i.session_id, i.session_uuid, i.run_id, i.seat, i.task,
       i.author, i.from_session, i.from_role, i.at, i.text, i.source_ref,
       i.repeat_count::int,
       i.report_filed,
       COALESCE(v.accepted, false)
FROM items i
LEFT JOIN verdicts v ON v.run_id = i.run_id AND v.task = i.task
$function$;

COMMENT ON FUNCTION memory.seat_memory(bigint) IS
    'Seat-tier memory for one project, for search and memory-keeper: one row per included seat item (brief, mail, peer message, report, thread entry, Stop-hook feedback), gated on the run having a report_filed event. Seat text is data, never operator speech. See the migration header for the rules.';
