-- Session memory becomes searchable project knowledge: thread log entries, answered seat
-- questions, the judge's unmet asks and corrections, session learnings, and each mail message a
-- session sent. Search matches their text through the indexes below and their meaning through
-- the embedding source view, which now lists them with the version each embedding must match:
--   thread_entry     work.thread_log rows are append-only, so the id is the version
--   question_answer  workspace.questions.settled_at
--   unmet_ask        the newest live verdict of a turn; a newer verdict is a new id
--   correction       the same, for verdicts whose reaction is corrected or redirected
--   learning         memory.session_knowledge.generated_at, keyed by sessions.sessions.id
--   mail             mailbox.messages rows are immutable once sent

SET LOCAL statement_timeout = 0;
SET LOCAL lock_timeout = '10s';

CREATE FUNCTION memory.session_learnings_text(learnings text[]) RETURNS text
    LANGUAGE sql IMMUTABLE PARALLEL SAFE
    RETURN array_to_string(learnings, E'\n');

CREATE INDEX idx_turn_verdicts_live_text ON memory.turn_verdicts
    USING gin (to_tsvector('english'::regconfig, unmet_ask || ' ' || rationale))
    WHERE mode = 'live';

CREATE INDEX idx_questions_text ON workspace.questions
    USING gin (to_tsvector('english'::regconfig, question || ' ' || coalesce(answer, '')));

ALTER TABLE work.thread_log
    ADD COLUMN search_tsv tsvector
        GENERATED ALWAYS AS (to_tsvector('english'::regconfig, summary || ' ' || body)) STORED;

CREATE INDEX idx_thread_log_search_tsv ON work.thread_log USING gin (search_tsv);

CREATE INDEX idx_turn_verdicts_live_turn_newest ON memory.turn_verdicts
    (operator_turn_id, created_at DESC, id DESC) WHERE mode = 'live';

CREATE INDEX idx_session_knowledge_learnings_text ON memory.session_knowledge
    USING gin (to_tsvector('english'::regconfig, memory.session_learnings_text(learnings)));

CREATE INDEX idx_mailbox_messages_sent_text ON mailbox.messages
    USING gin (search_tsv) WHERE from_session_id IS NOT NULL;

ALTER TABLE memory.knowledge_embedding_state
    DROP CONSTRAINT knowledge_embedding_state_source_kind_check,
    DROP CONSTRAINT knowledge_embedding_state_version_shape_check;

ALTER TABLE memory.knowledge_embedding_state
    ADD CONSTRAINT knowledge_embedding_state_source_kind_check
        CHECK (source_kind IN ('artifact', 'lesson', 'note', 'thread', 'thread_entry',
                               'question_answer', 'unmet_ask', 'correction', 'learning', 'mail'))
        NOT VALID,
    ADD CONSTRAINT knowledge_embedding_state_version_shape_check
        CHECK ((source_kind IN ('note', 'question_answer', 'learning') OR source_updated_at IS NULL)
           AND (source_kind = 'thread' OR source_seq IS NULL)
           AND (source_kind = 'lesson' OR source_hash IS NULL))
        NOT VALID;

ALTER TABLE memory.knowledge_embedding_state
    VALIDATE CONSTRAINT knowledge_embedding_state_source_kind_check;
ALTER TABLE memory.knowledge_embedding_state
    VALIDATE CONSTRAINT knowledge_embedding_state_version_shape_check;

CREATE VIEW memory.live_turn_verdicts_v AS
SELECT v.id, v.operator_turn_id, v.session_id, v.reaction, v.unmet_ask, v.rationale,
       v.created_at, t.project_id
  FROM memory.turn_verdicts v
  JOIN memory.operator_turns t ON t.id = v.operator_turn_id
 WHERE v.mode = 'live'
   AND NOT EXISTS (SELECT 1 FROM memory.turn_verdicts n
                    WHERE n.operator_turn_id = v.operator_turn_id AND n.mode = 'live'
                      AND (n.created_at, n.id) > (v.created_at, v.id));

COMMENT ON VIEW memory.live_turn_verdicts_v IS
    'The newest live judge verdict of each operator turn, with the turn''s project.';

CREATE OR REPLACE VIEW memory.knowledge_embedding_sources_v AS
SELECT 'artifact'::text AS source_kind, d.current_revision AS source_id,
       'artifact:' || d.current_revision AS reference, loc.project_id,
       NULL::timestamptz AS source_updated_at, NULL::bigint AS source_seq, NULL::text AS source_hash
  FROM memory.knowledge_documents d
  JOIN memory.knowledge_sources src ON src.source_uuid = d.source_uuid
  JOIN projects.locations loc ON loc.location_uuid = src.location_uuid
 WHERE d.deleted_at IS NULL
UNION ALL
SELECT 'lesson', l.id, 'lesson:' || l.uuid, l.project_id, NULL, NULL, l.text_md5
  FROM memory.lessons l
 WHERE l.project_id IS NOT NULL
UNION ALL
SELECT 'note', n.id, 'note:' || n.note_uuid, s.project_id, n.updated_at, NULL, NULL
  FROM workspace.session_notes n
  JOIN sessions.sessions s ON s.id = n.session_id
UNION ALL
SELECT 'thread', t.id, 'thread:' || t.thread_uuid, t.project_id, NULL, last.message_id, NULL
  FROM mailbox.threads t
  CROSS JOIN LATERAL (SELECT max(m.id) AS message_id
                        FROM mailbox.messages m WHERE m.thread_id = t.id) last
 WHERE t.project_id IS NOT NULL AND t.last_message_at < now() - interval '10 minutes'
UNION ALL
SELECT 'thread_entry', l.id, 'thread_entry:' || l.id, wt.project_id, NULL, NULL, NULL
  FROM work.thread_log l
  JOIN work.threads wt ON wt.id = l.thread_id
 WHERE l.kind IS DISTINCT FROM 'state-replaced'
UNION ALL
SELECT 'question_answer', q.id, 'question_answer:' || q.question_uuid, q.project_id,
       q.settled_at, NULL, NULL
  FROM workspace.questions q
 WHERE q.answer IS NOT NULL AND q.project_id IS NOT NULL
UNION ALL
SELECT 'unmet_ask', v.id, 'unmet_ask:' || v.id, v.project_id, NULL, NULL, NULL
  FROM memory.live_turn_verdicts_v v
 WHERE btrim(coalesce(v.unmet_ask, '')) <> '' AND v.project_id IS NOT NULL
UNION ALL
SELECT 'correction', v.id, 'correction:' || v.id, v.project_id, NULL, NULL, NULL
  FROM memory.live_turn_verdicts_v v
 WHERE v.reaction IN ('corrected', 'redirected') AND v.project_id IS NOT NULL
UNION ALL
SELECT 'learning', s.id, 'learning:' || k.session_id, s.project_id, k.generated_at, NULL, NULL
  FROM memory.session_knowledge k
  JOIN sessions.sessions s ON s.session_uuid = k.session_id
 WHERE cardinality(k.learnings) > 0 AND s.project_id IS NOT NULL
UNION ALL
SELECT 'mail', m.id, 'mail:' || m.message_uuid, th.project_id, NULL, NULL, NULL
  FROM mailbox.threads th
  JOIN mailbox.messages m ON m.thread_id = th.id
 WHERE th.project_id IS NOT NULL AND m.from_session_id IS NOT NULL
UNION ALL
SELECT 'mail', m.id, 'mail:' || m.message_uuid, fs.project_id, NULL, NULL, NULL
  FROM sessions.sessions fs
  JOIN mailbox.messages m ON m.from_session_id = fs.id
  LEFT JOIN mailbox.threads th ON th.id = m.thread_id
 WHERE fs.project_id IS NOT NULL AND th.project_id IS NULL;

COMMENT ON VIEW memory.knowledge_embedding_sources_v IS
    'Every source project knowledge embeds, with the version its embedding must match: artifacts by revision id, notes by updated_at, threads by newest message id, lessons by text_md5, answered questions by settled_at, session learnings by generated_at; thread entries, verdicts and mail by id. Bodies are read from the source tables only for the sources being embedded.';
