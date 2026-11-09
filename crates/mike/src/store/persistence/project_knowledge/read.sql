WITH source_freshness AS (
    SELECT s.source_uuid, jsonb_build_object(
               'state', CASE WHEN jsonb_array_length(fr.reasons) = 0 THEN 'fresh' ELSE 'stale' END,
               'reasons', fr.reasons,
               'last_scan_at', s.last_scan_at,
               'last_verified_at', GREATEST(s.last_scan_at, s.last_heartbeat_at)) AS freshness
      FROM memory.knowledge_sources s
      JOIN projects.locations sl ON sl.location_uuid = s.location_uuid
     CROSS JOIN LATERAL (
        SELECT COALESCE(jsonb_agg(f.reason), '[]'::jsonb) AS reasons
          FROM (VALUES
            (CASE WHEN s.last_failure IS NOT NULL AND s.last_failure_generation = s.generation
                  THEN 'the source''s latest scan failed: ' || s.last_failure END),
            (CASE WHEN GREATEST(s.last_scan_at, s.last_heartbeat_at) IS NULL
                  THEN 'the source has never been scanned'
                  WHEN GREATEST(s.last_scan_at, s.last_heartbeat_at) < now() - interval '1 hour'
                  THEN 'no scanner has checked the source since '
                       || to_char(GREATEST(s.last_scan_at, s.last_heartbeat_at) AT TIME ZONE 'UTC',
                                  'YYYY-MM-DD"T"HH24:MI:SS"Z"') END)
          ) AS f(reason)
         WHERE f.reason IS NOT NULL
     ) fr
     WHERE $2 = 'artifact' AND ($1::bigint IS NULL OR sl.project_id = $1)
), found(item, project_id) AS (
SELECT jsonb_build_object('reference', 'artifact:' || r.id, 'body', r.body,
    'title', r.title, 'status', r.status, 'kind', r.kind, 'metadata', r.metadata,
    'revision', r.id, 'content_hash', r.content_hash, 'observed_at', r.observed_at,
    'path', d.relative_path, 'root_path', s.root_path, 'last_scan_at', s.last_scan_at,
    'location_uuid', loc.location_uuid, 'machine_uuid', loc.machine_uuid,
    'deleted', d.deleted_at IS NOT NULL, 'current', d.current_revision = r.id,
    'freshness', sf.freshness), loc.project_id
FROM memory.knowledge_revisions r
JOIN memory.knowledge_documents d ON d.document_uuid = r.document_uuid
JOIN memory.knowledge_sources s ON s.source_uuid = d.source_uuid
JOIN projects.locations loc ON loc.location_uuid = s.location_uuid
JOIN source_freshness sf ON sf.source_uuid = s.source_uuid
WHERE ($1::bigint IS NULL OR loc.project_id = $1) AND $2 = 'artifact' AND r.id = $3
UNION ALL
SELECT jsonb_build_object('reference', 'lesson:' || l.uuid, 'body', l.text,
    'kind', k.code, 'status', st.code, 'topic', l.topic, 'expires_at', l.expires_at,
    'archived_at', l.archived_at, 'source_turn_id', l.source_turn_id), l.project_id
FROM memory.lessons l JOIN memory.lesson_kinds k ON k.id = l.kind_id
JOIN memory.lesson_stages st ON st.id = l.stage_id
WHERE ($1::bigint IS NULL OR l.project_id = $1) AND $2 = 'lesson' AND l.uuid = $4
UNION ALL
SELECT jsonb_build_object('reference', 'note:' || n.note_uuid, 'body', n.body,
    'title', n.title, 'status', n.status, 'kind', n.kind, 'data', n.data,
    'tags', n.tags, 'session_uuid', s.session_uuid, 'updated_at', n.updated_at), s.project_id
FROM workspace.session_notes n JOIN sessions.sessions s ON s.id = n.session_id
WHERE ($1::bigint IS NULL OR s.project_id = $1) AND $2 = 'note' AND n.note_uuid = $4
UNION ALL
SELECT jsonb_build_object('reference', 'turn:' || t.id, 'body', tx.final_assistant_text,
    'user_prompt', tx.user_prompt, 'session_uuid', s.session_uuid, 'created_at', t.created_at),
    t.project_id
FROM memory.operator_turns t JOIN sessions.sessions s ON s.id = t.session_id
JOIN mike.operator_turn_text_v tx ON tx.turn_id = t.id
WHERE ($1::bigint IS NULL OR t.project_id = $1) AND $2 = 'turn' AND t.id = $3
UNION ALL
SELECT jsonb_build_object('reference', 'thread:' || tx.thread_uuid, 'body', tx.body,
    'title', tx.subject, 'thread_uuid', tx.thread_uuid, 'fleet_uuid', tx.fleet_uuid,
    'resolved_at', tx.resolved_at, 'last_message_at', tx.last_message_at), tx.project_id
FROM mailbox.thread_text_v tx
WHERE ($1::bigint IS NULL OR tx.project_id = $1) AND $2 = 'thread' AND tx.thread_uuid = $4
UNION ALL
SELECT jsonb_build_object('reference', 'work_thread:' || wt.slug, 'slug', wt.slug,
    'title', wt.title, 'status', wt.status, 'goal', wt.goal,
    'tldr', wts.tldr, 'where_we_are', wts.where_we_are, 'next_up', wts.next_up,
    'blockers', wts.blockers, 'open_questions', wts.open_questions, 'decisions', wts.decisions,
    'acceptance_criteria', wtc.criteria_text, 'updated_at', wt.updated_at,
    'body', concat_ws(E'\n\n', 'Goal: ' || wt.goal, 'Summary: ' || wts.tldr,
      'Where we are: ' || wts.where_we_are, 'Next up: ' || wts.next_up,
      'Blockers: ' || wts.blockers, 'Open questions: ' || wts.open_questions,
      'Decisions: ' || wts.decisions, 'Acceptance criteria: ' || wtc.criteria_text)),
    wt.project_id
FROM work.threads wt
LEFT JOIN work.thread_state wts ON wts.thread_id = wt.id
LEFT JOIN work.thread_acceptance_criteria wtc ON wtc.thread_id = wt.id
WHERE ($1::bigint IS NULL OR wt.project_id = $1) AND $2 = 'work_thread' AND wt.slug = $5
UNION ALL
SELECT jsonb_build_object('reference', 'thread_entry:' || l.id,
    'body', concat_ws(E'\n\n', l.summary, l.body), 'summary', l.summary, 'entry_body', l.body,
    'entry_kind', l.kind, 'data', l.data, 'entry_date', l.entry_date, 'created_at', l.created_at,
    'commit_sha', l.commit_sha, 'session_uuid', l.session_uuid, 'slug', wt.slug,
    'thread_reference', 'work_thread:' || wt.slug), wt.project_id
FROM work.thread_log l JOIN work.threads wt ON wt.id = l.thread_id
WHERE ($1::bigint IS NULL OR wt.project_id = $1) AND $2 = 'thread_entry' AND l.id = $3
UNION ALL
SELECT jsonb_build_object('reference', 'question_answer:' || qa.question_uuid,
    'body', 'Question: ' || qa.question || E'\n\nAnswer: ' || COALESCE(qa.answer, ''),
    'question', qa.question, 'answer', qa.answer, 'state', qa.state, 'blocked', qa.blocked,
    'run_id', qa.run_id, 'to_task', qa.to_task, 'about_note_uuid', qa.about_note_uuid,
    'asker_session_uuid', asker.session_uuid, 'to_session_uuid', target.session_uuid,
    'settled_by_session_uuid', settler.session_uuid, 'created_at', qa.created_at,
    'settled_at', qa.settled_at), qa.project_id
FROM workspace.questions qa
LEFT JOIN sessions.sessions asker ON asker.id = qa.asker_session_id
LEFT JOIN sessions.sessions target ON target.id = qa.to_session_id
LEFT JOIN sessions.sessions settler ON settler.id = qa.settled_by_session_id
WHERE ($1::bigint IS NULL OR qa.project_id = $1) AND $2 = 'question_answer'
  AND qa.question_uuid = $4
UNION ALL
SELECT jsonb_build_object('reference', $2 || ':' || v.id,
    'body', concat_ws(E'\n\n', NULLIF(btrim(COALESCE(v.unmet_ask, '')), ''), v.rationale),
    'unmet_ask', v.unmet_ask, 'rationale', v.rationale, 'reaction', v.reaction,
    'delivery', v.delivery, 'evidence_quality', v.evidence_quality, 'severity', v.severity,
    'failure_tags', v.failure_tags, 'confidence', v.confidence, 'model', v.model,
    'mode', v.mode, 'created_at', v.created_at,
    'current', NOT EXISTS (SELECT 1 FROM memory.turn_verdicts n
                            WHERE n.operator_turn_id = v.operator_turn_id AND n.mode = 'live'
                              AND (n.created_at, n.id) > (v.created_at, v.id)),
    'turn_reference', 'turn:' || t.id, 'user_prompt', tx.user_prompt,
    'final_assistant_text', tx.final_assistant_text, 'session_uuid', s.session_uuid),
    t.project_id
FROM memory.turn_verdicts v
JOIN memory.operator_turns t ON t.id = v.operator_turn_id
JOIN sessions.sessions s ON s.id = t.session_id
LEFT JOIN mike.operator_turn_text_v tx ON tx.turn_id = t.id
WHERE ($1::bigint IS NULL OR t.project_id = $1) AND $2 IN ('unmet_ask', 'correction')
  AND v.id = $3
UNION ALL
SELECT jsonb_build_object('reference', 'learning:' || k.session_id,
    'body', memory.session_learnings_text(k.learnings), 'learnings', k.learnings,
    'topics', k.topics, 'related_session_ids', k.related_session_ids,
    'generated_at', k.generated_at, 'session_uuid', k.session_id, 'session_title', s.title),
    s.project_id
FROM memory.session_knowledge k JOIN sessions.sessions s ON s.session_uuid = k.session_id
WHERE ($1::bigint IS NULL OR s.project_id = $1) AND $2 = 'learning' AND k.session_id = $4
UNION ALL
SELECT jsonb_build_object('reference', 'mail:' || m.message_uuid, 'body', m.body,
    'message_uuid', m.message_uuid, 'thread_uuid', th.thread_uuid,
    'in_reply_to', reply.message_uuid, 'from_session_uuid', fs.session_uuid,
    'from_session_title', fs.title, 'to_session_uuid', ts.session_uuid,
    'to_session_title', ts.title, 'queued_at', m.queued_at, 'sent_at', m.sent_at,
    'channel_delivered_at', m.channel_delivered_at, 'consumed_at', m.consumed_at,
    'dead_lettered_at', m.dead_lettered_at), COALESCE(th.project_id, fs.project_id)
FROM mailbox.messages m
LEFT JOIN mailbox.threads th ON th.id = m.thread_id
LEFT JOIN sessions.sessions fs ON fs.id = m.from_session_id
LEFT JOIN sessions.sessions ts ON ts.id = m.to_session_id
LEFT JOIN mailbox.messages reply ON reply.id = m.in_reply_to_id
WHERE $2 = 'mail' AND m.message_uuid = $4
  AND ($1::bigint IS NULL OR COALESCE(th.project_id, fs.project_id) = $1)
)
SELECT f.item || jsonb_build_object('project_uuid', p.project_uuid)
  FROM found f JOIN projects.projects p ON p.id = f.project_id
