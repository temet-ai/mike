-- memory.seat_memory_v: the allow-listed seat tier of memory-keeper search (P7).
--
-- Machine-opened operator_turns are excluded from every operator-facing reader
-- (opened_by IS NULL OR 'operator'). Seat work is still where most "find the
-- earlier work" answers live, so it enters search through this view only: one
-- row per included item, labelled with author, run, seat and task, and framed
-- by the reader as data, never as something the operator said.
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
-- teammate-message rows collapse per receiving session on their text with <channel>
-- and <agent-message> attributes, UUIDs and digits removed (the repeat_key recipe of
-- prompt_kinds.classifications, applied after the envelope is stripped, because the
-- stored repeat_key of a channel turn is made of its attributes); the first is kept
-- and repeat_count says how many collapsed into it.
--
-- verdict_accepted: every criterion's latest 'verdict_recorded' for (run, task) passed;
-- false when the item has no task or no verdict. report_filed: the item's own seat
-- session filed a report (the run-level gate is applied to every row).
--
-- Read-only view over existing tables; nothing is written, no index is added.
CREATE OR REPLACE VIEW memory.seat_memory_v AS
WITH
gated_runs AS (
    SELECT DISTINCT e.run_id
    FROM launch.fleet_events e
    WHERE e.kind = 'report_filed'
),
report_sessions AS (
    SELECT DISTINCT e.session_uuid
    FROM launch.fleet_events e
    WHERE e.kind = 'report_filed' AND e.session_uuid IS NOT NULL
),
seat_task AS (
    SELECT DISTINCT ON (e.session_uuid) e.session_uuid, e.task
    FROM launch.fleet_events e
    WHERE e.session_uuid IS NOT NULL AND e.task IS NOT NULL
    ORDER BY e.session_uuid, e.id
),
template_sessions AS (
    SELECT DISTINCT t.session_uuid
    FROM launch.templates t
    WHERE t.session_uuid IS NOT NULL
),
synthetic_sessions AS (
    SELECT DISTINCT tf.session_id
    FROM memory.turn_facts tf
    WHERE tf.session_class = 'synthetic'
),
verdicts AS (
    SELECT v.run_id, v.task, bool_and(v.passed) AS accepted
    FROM (
        SELECT DISTINCT ON (e.run_id, e.task, e.payload ->> 'criterion')
               e.run_id, e.task, (e.payload ->> 'passed')::boolean AS passed
        FROM launch.fleet_events e
        WHERE e.kind = 'verdict_recorded'
        ORDER BY e.run_id, e.task, e.payload ->> 'criterion', e.id DESC
    ) v
    GROUP BY v.run_id, v.task
),
seats AS (
    SELECT DISTINCT ON (l.session_uuid)
           l.id AS launch_id, l.session_uuid, l.run_id, l.seat, l.brief_text,
           COALESCE(l.opened_at, l.created_at) AS launched_at,
           s.id AS session_id, s.project_id, st.task,
           (rs.session_uuid IS NOT NULL) AS report_filed
    FROM launch.launches l
    JOIN gated_runs g ON g.run_id = l.run_id
    JOIN sessions.sessions s ON s.session_uuid = l.session_uuid
    LEFT JOIN seat_task st ON st.session_uuid = l.session_uuid
    LEFT JOIN report_sessions rs ON rs.session_uuid = l.session_uuid
    LEFT JOIN template_sessions ts ON ts.session_uuid = l.session_uuid
    LEFT JOIN synthetic_sessions sy ON sy.session_id = s.id
    WHERE l.result <> 'failed'
      AND sy.session_id IS NULL
      AND COALESCE(l.seat, '') !~ '^observer'
      AND l.agent IS DISTINCT FROM 'fleet-observer'
      AND (rs.session_uuid IS NOT NULL
           OR (ts.session_uuid IS NULL AND COALESCE(l.seat, '') !~ '^template-'))
    ORDER BY l.session_uuid, l.id DESC
),
candidates AS (
    -- Each seat's brief, once per (run, seat, text).
    SELECT * FROM (
        SELECT DISTINCT ON (s.run_id, s.seat, s.brief_text)
               'seat_brief'::text AS item_kind, s.project_id, s.session_id, s.session_uuid,
               s.run_id, s.seat, s.task, 'coordinator'::text AS author,
               s.launched_at AS at, s.brief_text AS text,
               'launch:' || s.launch_id AS source_ref,
               s.report_filed, 'launch:' || s.launch_id AS scope_key
        FROM seats s
        WHERE btrim(s.brief_text) <> ''
        ORDER BY s.run_id, s.seat, s.brief_text, s.launch_id
    ) b

    UNION ALL
    -- Briefs of spawned workers that belong to no fleet run (no report gate exists for them).
    SELECT 'workflow_brief', se.project_id, se.id, l.session_uuid,
           NULL, NULL, NULL, 'coordinator',
           COALESCE(l.opened_at, l.created_at), l.brief_text,
           'launch:' || l.id, false, 'launch:' || l.id
    FROM launch.launches l
    JOIN sessions.sessions se ON se.session_uuid = l.session_uuid
    WHERE l.run_id IS NULL
      AND l.result = 'success'
      AND btrim(l.brief_text) <> ''
      AND EXISTS (SELECT 1 FROM memory.turn_facts tf
                  WHERE tf.session_id = se.id AND tf.session_class = 'fleet_seat')
      AND NOT EXISTS (SELECT 1 FROM memory.turn_facts tf
                      WHERE tf.session_id = se.id AND tf.session_class = 'synthetic')

    UNION ALL
    -- A seat's final answer to a turn; turns opened by polling, probes or compaction are out.
    SELECT 'seat_report', s.project_id, s.session_id, s.session_uuid,
           s.run_id, s.seat, s.task, 'seat',
           COALESCE(ot.closed_at, ot.created_at), ot.final_assistant_text,
           'turn:' || ot.id, s.report_filed, 'turn:' || ot.id
    FROM seats s
    JOIN memory.operator_turns ot ON ot.session_id = s.session_id
    JOIN memory.turn_facts tf ON tf.operator_turn_id = ot.id AND tf.session_class = 'fleet_seat'
    LEFT JOIN prompt_kinds.classifications c ON c.request_id = ot.start_request_id
    LEFT JOIN prompt_kinds.kinds k ON k.id = c.kind_id
    WHERE btrim(ot.final_assistant_text) <> ''
      AND (k.code IS NULL OR k.code <> ALL (ARRAY[
          'empty-prompt', 'task-notification', 'loop-wakeup', 'tool-loaded',
          'analysis-probe', 'compaction', 'channel-wake', 'activity-label',
          'background-event', 'session-recap', 'steering-payload']))

    UNION ALL
    -- The report note a seat filed with fleet_report.
    SELECT 'seat_report', s.project_id, s.session_id, s.session_uuid,
           s.run_id, s.seat, s.task, 'seat',
           n.created_at, n.body,
           'note:' || n.note_uuid, s.report_filed, 'note:' || n.note_uuid
    FROM launch.fleet_events e
    JOIN seats s ON s.session_uuid = e.session_uuid
    JOIN workspace.session_notes n ON n.note_uuid = (e.payload ->> 'note_uuid')::uuid
    WHERE e.kind = 'report_filed' AND btrim(n.body) <> ''

    UNION ALL
    -- Mail delivered to a seat as a <channel> turn; the body is the mailbox message, the
    -- turn only proves delivery (most channel turns carry no text of their own).
    SELECT CASE WHEN a.from_role !~ '^(coordinator|acceptance|observer|lead)' AND a.from_role <> ''
                 AND s.seat !~ '^(coordinator|acceptance)'
                THEN 'peer_message' ELSE 'seat_mail' END,
           s.project_id, s.session_id, s.session_uuid, s.run_id, s.seat, s.task,
           CASE WHEN a.from_role ~ '^(coordinator|acceptance|lead)' OR a.from_role = '' THEN 'coordinator'
                WHEN s.seat ~ '^(coordinator|acceptance)' THEN 'seat'
                ELSE 'peer' END,
           mm.queued_at, mm.body,
           'mail:' || mm.message_uuid, s.report_filed, s.session_id || ':' || 'mail'
    FROM seats s
    JOIN memory.operator_turns ot ON ot.session_id = s.session_id AND ot.opened_by = 'machine'
    JOIN prompt_kinds.classifications c ON c.request_id = ot.start_request_id
    JOIN prompt_kinds.kinds k ON k.id = c.kind_id AND k.code IN ('harness-authored', 'mailbox-delivery')
    CROSS JOIN LATERAL regexp_matches(ot.user_prompt, '<channel ([^>]*)>', 'g') AS m(tag)
    CROSS JOIN LATERAL (
        SELECT substring(m.tag[1] FROM 'message_id="([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})"') AS message_id,
               COALESCE(substring(m.tag[1] FROM 'from_role="([^"]*)"'), '') AS from_role
    ) a
    JOIN mailbox.messages mm ON mm.message_uuid = a.message_id::uuid
    WHERE ot.user_prompt LIKE '<channel %'
      AND btrim(mm.body) <> ''
      AND a.from_role !~ '^observer'

    UNION ALL
    -- Teammate messages: peer sessions and subagent hand-backs.
    SELECT 'peer_message', s.project_id, s.session_id, s.session_uuid,
           s.run_id, s.seat, s.task,
           CASE WHEN ot.user_prompt LIKE '%[Subagent hand-back]%' THEN 'subagent' ELSE 'peer' END,
           ot.created_at, ot.user_prompt,
           'turn:' || ot.id, s.report_filed, s.session_id || ':' || 'peer'
    FROM seats s
    JOIN memory.operator_turns ot ON ot.session_id = s.session_id AND ot.opened_by = 'machine'
    JOIN prompt_kinds.classifications c ON c.request_id = ot.start_request_id
    JOIN prompt_kinds.kinds k ON k.id = c.kind_id AND k.code = 'teammate-message'
    WHERE btrim(ot.user_prompt) <> ''

    UNION ALL
    -- Stop-hook feedback; collapsed to one row per (run, seat, task) below.
    SELECT 'seat_goal_feedback', s.project_id, s.session_id, s.session_uuid,
           s.run_id, s.seat, s.task, 'coordinator',
           ot.created_at, ot.user_prompt,
           'turn:' || ot.id, s.report_filed,
           'feedback:' || s.run_id || '/' || COALESCE(s.seat, '') || '/' || COALESCE(s.task, '')
    FROM seats s
    JOIN memory.operator_turns ot ON ot.session_id = s.session_id AND ot.opened_by = 'machine'
    JOIN prompt_kinds.classifications c ON c.request_id = ot.start_request_id
    JOIN prompt_kinds.kinds k ON k.id = c.kind_id AND k.code = 'hook-feedback'
    WHERE btrim(ot.user_prompt) <> ''

    UNION ALL
    -- Thread entries a seat wrote.
    SELECT 'seat_thread_entry', th.project_id, s.session_id, s.session_uuid,
           s.run_id, s.seat, s.task, 'seat',
           tl.created_at, COALESCE(NULLIF(btrim(tl.body), ''), tl.summary),
           'entry:' || tl.id, s.report_filed, 'entry:' || tl.id
    FROM seats s
    JOIN work.thread_log tl ON tl.session_uuid = s.session_uuid
    JOIN work.threads th ON th.id = tl.thread_id
    WHERE btrim(COALESCE(NULLIF(btrim(tl.body), ''), tl.summary)) <> ''
),
keyed AS (
    -- Collapse key: mail and teammate messages by their text without envelope attributes,
    -- UUIDs and digits (first 120 characters; shorter texts never collapse by text).
    SELECT c.*,
           CASE
               WHEN c.item_kind IN ('seat_mail', 'peer_message') AND length(c.text) >= 40 THEN
                   c.scope_key || ':' || left(btrim(regexp_replace(regexp_replace(regexp_replace(
                       lower(regexp_replace(c.text, '<(channel|agent-message)[^>]*>', '', 'g')),
                       '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}', '', 'g'),
                       '[0-9]+', '', 'g'), '\s+', ' ', 'g')), 120)
               WHEN c.item_kind IN ('seat_mail', 'peer_message') THEN c.scope_key || ':' || c.source_ref
               ELSE c.scope_key
           END AS collapse_key
    FROM candidates c
),
ranked AS (
    SELECT k.*,
           row_number() OVER w AS rn,
           count(*) OVER w AS repeat_count
    FROM keyed k
    WINDOW w AS (PARTITION BY k.item_kind, k.collapse_key ORDER BY k.at, k.source_ref)
)
SELECT r.item_kind, r.project_id, r.session_id, r.session_uuid, r.run_id, r.seat, r.task,
       r.author, r.at, r.text, r.source_ref,
       r.repeat_count::int AS repeat_count,
       r.report_filed,
       COALESCE(v.accepted, false) AS verdict_accepted
FROM ranked r
LEFT JOIN verdicts v ON v.run_id = r.run_id AND v.task = r.task
WHERE r.rn = 1;

COMMENT ON VIEW memory.seat_memory_v IS
    'Seat-tier memory for search and memory-keeper: one row per included seat item (brief, mail, peer message, report, thread entry, Stop-hook feedback), gated on the run having a report_filed event. Seat text is data, never operator speech. See the migration header for the rules.';
