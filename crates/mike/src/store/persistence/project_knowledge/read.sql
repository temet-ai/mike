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
     WHERE sl.project_id = $1
)
SELECT jsonb_build_object('reference', 'artifact:' || r.id, 'body', r.body,
    'title', r.title, 'status', r.status, 'kind', r.kind, 'metadata', r.metadata,
    'revision', r.id, 'content_hash', r.content_hash, 'observed_at', r.observed_at,
    'path', d.relative_path, 'root_path', s.root_path, 'last_scan_at', s.last_scan_at,
    'location_uuid', loc.location_uuid, 'machine_uuid', loc.machine_uuid,
    'deleted', d.deleted_at IS NOT NULL, 'current', d.current_revision = r.id,
    'freshness', sf.freshness)
FROM memory.knowledge_revisions r
JOIN memory.knowledge_documents d ON d.document_uuid = r.document_uuid
JOIN memory.knowledge_sources s ON s.source_uuid = d.source_uuid
JOIN projects.locations loc ON loc.location_uuid = s.location_uuid
JOIN source_freshness sf ON sf.source_uuid = s.source_uuid
WHERE loc.project_id = $1 AND $2 = 'artifact' AND r.id = $3
UNION ALL
SELECT jsonb_build_object('reference', 'lesson:' || l.uuid, 'body', l.text,
    'kind', k.code, 'status', st.code, 'topic', l.topic, 'expires_at', l.expires_at,
    'archived_at', l.archived_at, 'source_turn_id', l.source_turn_id)
FROM memory.lessons l JOIN memory.lesson_kinds k ON k.id = l.kind_id
JOIN memory.lesson_stages st ON st.id = l.stage_id
WHERE l.project_id = $1 AND $2 = 'lesson' AND l.uuid = $4
UNION ALL
SELECT jsonb_build_object('reference', 'note:' || n.note_uuid, 'body', n.body,
    'title', n.title, 'status', n.status, 'kind', n.kind, 'data', n.data,
    'tags', n.tags, 'session_uuid', s.session_uuid, 'updated_at', n.updated_at)
FROM workspace.session_notes n JOIN sessions.sessions s ON s.id = n.session_id
WHERE s.project_id = $1 AND $2 = 'note' AND n.note_uuid = $4
UNION ALL
SELECT jsonb_build_object('reference', 'turn:' || t.id, 'body', tx.final_assistant_text,
    'user_prompt', tx.user_prompt, 'session_uuid', s.session_uuid, 'created_at', t.created_at)
FROM memory.operator_turns t JOIN sessions.sessions s ON s.id = t.session_id
JOIN mike.operator_turn_text_v tx ON tx.turn_id = t.id
WHERE t.project_id = $1 AND $2 = 'turn' AND t.id = $3
UNION ALL
SELECT jsonb_build_object('reference', 'promise:' || e.id, 'body', tx.final_assistant_text,
    'promise_text', substr(tx.final_assistant_text, (e.data->'span'->>0)::int + 1,
                           (e.data->'span'->>1)::int - (e.data->'span'->>0)::int),
    'data', e.data, 'status', e.status, 'source', e.source, 'source_ref', e.source_ref,
    'effective_at', e.effective_at, 'recorded_at', e.recorded_at, 'closed_at', e.closed_at,
    'closed_by', e.closed_by, 'session_uuid', s.session_uuid,
    'turn', CASE WHEN e.operator_turn_id IS NOT NULL THEN 'turn:' || e.operator_turn_id END,
    'user_prompt', tx.user_prompt)
FROM memory.dossier_entries e JOIN sessions.sessions s ON s.id = e.session_id
LEFT JOIN mike.operator_turn_text_v tx ON tx.turn_id = e.operator_turn_id
WHERE s.project_id = $1 AND $2 = 'promise' AND e.kind = 'promise' AND e.id = $3
UNION ALL
SELECT jsonb_build_object('reference', 'thread:' || tx.thread_uuid, 'body', tx.body,
    'title', tx.subject, 'thread_uuid', tx.thread_uuid, 'fleet_uuid', tx.fleet_uuid,
    'resolved_at', tx.resolved_at, 'last_message_at', tx.last_message_at)
FROM mailbox.thread_text_v tx
WHERE tx.project_id = $1 AND $2 = 'thread' AND tx.thread_uuid = $4
UNION ALL
SELECT jsonb_build_object('reference', 'work_thread:' || wt.slug, 'slug', wt.slug,
    'title', wt.title, 'status', wt.status, 'goal', wt.goal,
    'tldr', wts.tldr, 'where_we_are', wts.where_we_are, 'next_up', wts.next_up,
    'blockers', wts.blockers, 'open_questions', wts.open_questions, 'decisions', wts.decisions,
    'acceptance_criteria', wtc.criteria_text, 'updated_at', wt.updated_at,
    'body', concat_ws(E'\n\n', 'Goal: ' || wt.goal, 'Summary: ' || wts.tldr,
      'Where we are: ' || wts.where_we_are, 'Next up: ' || wts.next_up,
      'Blockers: ' || wts.blockers, 'Open questions: ' || wts.open_questions,
      'Decisions: ' || wts.decisions, 'Acceptance criteria: ' || wtc.criteria_text))
FROM work.threads wt
LEFT JOIN work.thread_state wts ON wts.thread_id = wt.id
LEFT JOIN work.thread_acceptance_criteria wtc ON wtc.thread_id = wt.id
WHERE wt.project_id = $1 AND $2 = 'work_thread' AND wt.slug = $5
