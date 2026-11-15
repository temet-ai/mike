CREATE TABLE memory.knowledge_kinds (
    kind text PRIMARY KEY CHECK (kind ~ '^[a-z][a-z0-9_]{0,39}$'),
    body_view text NOT NULL,
    settle_secs integer NOT NULL DEFAULT 0 CHECK (settle_secs BETWEEN 0 AND 86400),
    enabled boolean NOT NULL DEFAULT true,
    built_in boolean NOT NULL DEFAULT false,
    created_at timestamptz NOT NULL DEFAULT now()
);

COMMENT ON TABLE memory.knowledge_kinds IS
    'One row per kind of project knowledge that is embedded. body_view names a view with the columns source_id bigint, project_id bigint, reference text, label text, name text, body text; settle_secs delays embedding after each change.';

CREATE TABLE memory.knowledge_kind_watches (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    kind text NOT NULL REFERENCES memory.knowledge_kinds(kind) ON DELETE CASCADE,
    watched_table text NOT NULL,
    key_column text NOT NULL,
    update_columns text[] NOT NULL DEFAULT '{}',
    via_table text,
    via_match_column text,
    via_key_column text,
    CHECK ((via_table IS NULL) = (via_match_column IS NULL) AND (via_table IS NULL) = (via_key_column IS NULL)),
    UNIQUE (kind, watched_table, key_column)
);

COMMENT ON TABLE memory.knowledge_kind_watches IS
    'A table whose writes change items of a kind. key_column holds the item''s source_id, or, with via_*, a value matched against via_table.via_match_column whose via_key_column is the source_id. memory.attach_knowledge_watch creates the trigger.';

CREATE TABLE memory.knowledge_items (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    kind text NOT NULL REFERENCES memory.knowledge_kinds(kind) ON DELETE CASCADE,
    source_id bigint NOT NULL,
    project_id bigint REFERENCES projects.projects(id) ON DELETE SET NULL,
    needs_embedding boolean NOT NULL DEFAULT true,
    ready_at timestamptz NOT NULL DEFAULT now(),
    attempts integer NOT NULL DEFAULT 0,
    last_error text,
    model_key text,
    changed_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    UNIQUE (kind, source_id)
);

COMMENT ON TABLE memory.knowledge_items IS
    'One row per embeddable source. Watch triggers insert or mark a row when its source changes; the knowledge sweep embeds rows with needs_embedding and clears it.';

CREATE INDEX idx_knowledge_items_waiting ON memory.knowledge_items (ready_at, id) WHERE needs_embedding;
CREATE INDEX idx_knowledge_items_failed ON memory.knowledge_items (kind) WHERE last_error IS NOT NULL;
CREATE INDEX idx_knowledge_items_project ON memory.knowledge_items (project_id);

CREATE FUNCTION memory.track_knowledge_item() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    v_kind text := TG_ARGV[0];
    v_key_column text := TG_ARGV[1];
    v_via_table text := TG_ARGV[2];
    v_via_match text := TG_ARGV[3];
    v_via_key text := TG_ARGV[4];
    v_via_type text := TG_ARGV[5];
    v_settle integer;
    v_keys text[] := ARRAY[]::text[];
    v_ids bigint[];
BEGIN
    SELECT k.settle_secs INTO v_settle FROM memory.knowledge_kinds k WHERE k.kind = v_kind AND k.enabled;
    IF NOT FOUND THEN
        RETURN NULL;
    END IF;
    IF TG_OP IN ('INSERT', 'UPDATE') THEN
        v_keys := v_keys || (to_jsonb(NEW) ->> v_key_column);
    END IF;
    IF TG_OP IN ('UPDATE', 'DELETE') THEN
        v_keys := v_keys || (to_jsonb(OLD) ->> v_key_column);
    END IF;
    IF v_via_table = '' THEN
        SELECT array_agg(DISTINCT k::bigint) INTO v_ids FROM unnest(v_keys) k WHERE k IS NOT NULL;
    ELSE
        EXECUTE format('SELECT array_agg(DISTINCT w.%I::bigint) FROM %s w WHERE w.%I = ANY($1::%s[])',
                       v_via_key, v_via_table, v_via_match, v_via_type)
           INTO v_ids USING v_keys;
    END IF;
    IF v_ids IS NULL THEN
        RETURN NULL;
    END IF;
    WITH present AS (
        SELECT DISTINCT ON (b.source_id) b.source_id, b.project_id
          FROM memory.knowledge_item_bodies(v_kind, v_ids) b
    ), gone AS (
        DELETE FROM memory.knowledge_items i
         WHERE i.kind = v_kind AND i.source_id = ANY(v_ids)
           AND NOT EXISTS (SELECT 1 FROM present p WHERE p.source_id = i.source_id)
    )
    INSERT INTO memory.knowledge_items AS i (kind, source_id, project_id, needs_embedding, ready_at)
    SELECT v_kind, p.source_id, p.project_id, true, now() + make_interval(secs => v_settle)
      FROM present p
    ON CONFLICT (kind, source_id) DO UPDATE
       SET needs_embedding = true, ready_at = EXCLUDED.ready_at, project_id = EXCLUDED.project_id,
           attempts = 0, last_error = NULL, changed_at = clock_timestamp();
    RETURN NULL;
END
$$;

CREATE FUNCTION memory.knowledge_watch_trigger_name(p_watch bigint) RETURNS text
LANGUAGE sql IMMUTABLE AS $$ SELECT 'trg_knowledge_item_' || p_watch $$;

CREATE FUNCTION memory.attach_knowledge_watch(p_watch bigint) RETURNS void
LANGUAGE plpgsql AS $$
DECLARE
    w memory.knowledge_kind_watches;
    v_table regclass;
    v_via regclass;
    v_via_type text := '';
    v_missing text;
    v_update text := '';
BEGIN
    SELECT * INTO STRICT w FROM memory.knowledge_kind_watches WHERE id = p_watch;
    v_table := to_regclass(w.watched_table);
    IF v_table IS NULL THEN
        RAISE EXCEPTION 'knowledge watch %: table % does not exist', p_watch, w.watched_table;
    END IF;
    SELECT c INTO v_missing FROM unnest(w.update_columns || w.key_column) c
     WHERE NOT EXISTS (SELECT 1 FROM pg_attribute a WHERE a.attrelid = v_table AND a.attname = c
                          AND a.attnum > 0 AND NOT a.attisdropped)
     LIMIT 1;
    IF v_missing IS NOT NULL THEN
        RAISE EXCEPTION 'knowledge watch %: column % is not on %', p_watch, v_missing, w.watched_table;
    END IF;
    IF w.via_table IS NOT NULL THEN
        v_via := to_regclass(w.via_table);
        IF v_via IS NULL THEN
            RAISE EXCEPTION 'knowledge watch %: table % does not exist', p_watch, w.via_table;
        END IF;
        SELECT format_type(a.atttypid, a.atttypmod) INTO v_via_type FROM pg_attribute a
         WHERE a.attrelid = v_via AND a.attname = w.via_match_column AND a.attnum > 0 AND NOT a.attisdropped;
        IF v_via_type IS NULL OR NOT EXISTS (SELECT 1 FROM pg_attribute a WHERE a.attrelid = v_via
                AND a.attname = w.via_key_column AND a.attnum > 0 AND NOT a.attisdropped) THEN
            RAISE EXCEPTION 'knowledge watch %: % lacks % or %', p_watch, w.via_table, w.via_match_column, w.via_key_column;
        END IF;
    END IF;
    IF cardinality(w.update_columns) > 0 THEN
        SELECT ' OR UPDATE OF ' || string_agg(quote_ident(c), ', ') INTO v_update FROM unnest(w.update_columns) c;
    END IF;
    EXECUTE format('DROP TRIGGER IF EXISTS %I ON %s', memory.knowledge_watch_trigger_name(p_watch), v_table);
    EXECUTE format(
        'CREATE TRIGGER %I AFTER INSERT OR DELETE%s ON %s FOR EACH ROW
         EXECUTE FUNCTION memory.track_knowledge_item(%L, %L, %L, %L, %L, %L)',
        memory.knowledge_watch_trigger_name(p_watch), v_update, v_table, w.kind, w.key_column,
        COALESCE(v_via::text, ''), COALESCE(w.via_match_column, ''), COALESCE(w.via_key_column, ''), v_via_type);
END
$$;

CREATE FUNCTION memory.detach_knowledge_watch(p_watch bigint) RETURNS void
LANGUAGE plpgsql AS $$
DECLARE
    v_table regclass;
BEGIN
    SELECT to_regclass(w.watched_table) INTO v_table FROM memory.knowledge_kind_watches w WHERE w.id = p_watch;
    IF v_table IS NOT NULL THEN
        EXECUTE format('DROP TRIGGER IF EXISTS %I ON %s', memory.knowledge_watch_trigger_name(p_watch), v_table);
    END IF;
END
$$;

CREATE FUNCTION memory.knowledge_item_bodies(p_kind text, p_ids bigint[])
RETURNS TABLE (source_id bigint, project_id bigint, reference text, label text, name text, body text)
LANGUAGE plpgsql STABLE AS $$
DECLARE
    v_view regclass;
BEGIN
    SELECT to_regclass(k.body_view) INTO v_view FROM memory.knowledge_kinds k WHERE k.kind = p_kind;
    IF v_view IS NULL THEN
        RETURN;
    END IF;
    RETURN QUERY EXECUTE format(
        'SELECT b.source_id::bigint, b.project_id::bigint, b.reference::text, b.label::text, b.name::text, b.body::text
           FROM %s b WHERE b.source_id = ANY($1)', v_view)
    USING p_ids;
END
$$;

CREATE FUNCTION memory.reconcile_knowledge_items(p_kind text) RETURNS bigint
LANGUAGE plpgsql AS $$
DECLARE
    v_view regclass;
    v_added bigint;
    v_marked bigint;
BEGIN
    SELECT to_regclass(k.body_view) INTO v_view FROM memory.knowledge_kinds k WHERE k.kind = p_kind AND k.enabled;
    IF v_view IS NULL THEN
        RETURN 0;
    END IF;
    EXECUTE format(
        'INSERT INTO memory.knowledge_items (kind, source_id, project_id)
         SELECT DISTINCT ON (b.source_id) $1, b.source_id, b.project_id FROM %s b
          WHERE NOT EXISTS (SELECT 1 FROM memory.knowledge_items i WHERE i.kind = $1 AND i.source_id = b.source_id)
         ON CONFLICT (kind, source_id) DO NOTHING', v_view)
    USING p_kind;
    GET DIAGNOSTICS v_added = ROW_COUNT;
    EXECUTE format(
        'UPDATE memory.knowledge_items i SET needs_embedding = true, ready_at = now(), changed_at = clock_timestamp()
           FROM memory.knowledge_embedding_state e
           LEFT JOIN (SELECT DISTINCT ON (b.source_id) b.source_id, b.project_id FROM %s b) cur
             ON cur.source_id = e.source_id
          WHERE i.kind = $1 AND e.item_id = i.id AND NOT i.needs_embedding
            AND (cur.source_id IS NULL OR cur.project_id IS DISTINCT FROM e.project_id)', v_view)
    USING p_kind;
    GET DIAGNOSTICS v_marked = ROW_COUNT;
    RETURN v_added + v_marked;
END
$$;

CREATE VIEW memory.knowledge_body_artifact_v AS
SELECT r.id AS source_id, loc.project_id, 'artifact:' || r.id AS reference,
       CASE src.name WHEN 'claude-memory' THEN 'memory' WHEN 'skills' THEN 'skill' ELSE 'artifact' END AS label,
       r.title AS name, r.title || E'\n\n' || r.body AS body
  FROM memory.knowledge_documents d
  JOIN memory.knowledge_revisions r ON r.id = d.current_revision
  JOIN memory.knowledge_sources src ON src.source_uuid = d.source_uuid
  JOIN projects.locations loc ON loc.location_uuid = src.location_uuid
 WHERE d.deleted_at IS NULL;

CREATE VIEW memory.knowledge_body_lesson_v AS
SELECT l.id AS source_id, l.project_id, 'lesson:' || l.uuid AS reference,
       concat_ws(', ', 'lesson', k.code, NULLIF(btrim(l.topic), '')) AS label, ls.title AS name, l.text AS body
  FROM memory.lessons l
  JOIN memory.lesson_kinds k ON k.id = l.kind_id
  LEFT JOIN memory.operator_turns lt ON lt.id = l.source_turn_id
  LEFT JOIN sessions.sessions ls ON ls.id = COALESCE(l.session_id, lt.session_id)
 WHERE l.project_id IS NOT NULL;

CREATE VIEW memory.knowledge_body_note_v AS
SELECT n.id AS source_id, s.project_id, 'note:' || n.note_uuid AS reference,
       concat_ws(', ', 'note', NULLIF(btrim(n.kind), '')) AS label,
       COALESCE(NULLIF(btrim(n.title), ''), s.title) AS name, n.title || E'\n\n' || n.body AS body
  FROM workspace.session_notes n
  JOIN sessions.sessions s ON s.id = n.session_id;

CREATE VIEW memory.knowledge_body_thread_v AS
SELECT tx.thread_id AS source_id, tx.project_id, 'thread:' || tx.thread_uuid AS reference,
       'thread'::text AS label, tx.subject AS name, tx.body
  FROM mailbox.thread_text_v tx
 WHERE tx.project_id IS NOT NULL;

CREATE VIEW memory.knowledge_body_thread_entry_v AS
SELECT l.id AS source_id, wt.project_id, 'thread_entry:' || l.id AS reference,
       'thread_entry'::text AS label, wt.slug AS name,
       concat_ws(E'\n\n', 'Work thread ' || wt.slug || ': ' || l.kind, l.summary, l.body) AS body
  FROM work.thread_log l
  JOIN work.threads wt ON wt.id = l.thread_id
 WHERE l.kind IS DISTINCT FROM 'state-replaced';

CREATE VIEW memory.knowledge_body_question_answer_v AS
SELECT q.id AS source_id, q.project_id, 'question_answer:' || q.question_uuid AS reference,
       'question_answer'::text AS label, COALESCE(settler.title, asker.title) AS name,
       concat_ws(E'\n\n', q.question, q.answer) AS body
  FROM workspace.questions q
  LEFT JOIN sessions.sessions asker ON asker.id = q.asker_session_id
  LEFT JOIN sessions.sessions settler ON settler.id = q.settled_by_session_id
 WHERE q.answer IS NOT NULL AND q.project_id IS NOT NULL;

CREATE VIEW memory.knowledge_body_unmet_ask_v AS
SELECT v.id AS source_id, v.project_id, 'unmet_ask:' || v.id AS reference,
       'unmet_ask'::text AS label, vs.title AS name,
       concat_ws(E'\n\n', t.user_prompt, v.unmet_ask, v.rationale) AS body
  FROM memory.live_turn_verdicts_v v
  JOIN memory.operator_turns t ON t.id = v.operator_turn_id
  LEFT JOIN sessions.sessions vs ON vs.id = v.session_id
 WHERE btrim(COALESCE(v.unmet_ask, '')) <> '' AND v.project_id IS NOT NULL;

CREATE VIEW memory.knowledge_body_correction_v AS
SELECT v.id AS source_id, v.project_id, 'correction:' || v.id AS reference,
       'correction'::text AS label, vs.title AS name,
       concat_ws(E'\n\n', t.user_prompt, v.unmet_ask, v.rationale) AS body
  FROM memory.live_turn_verdicts_v v
  JOIN memory.operator_turns t ON t.id = v.operator_turn_id
  LEFT JOIN sessions.sessions vs ON vs.id = v.session_id
 WHERE v.reaction IN ('corrected', 'redirected') AND v.project_id IS NOT NULL;

CREATE VIEW memory.knowledge_body_learning_v AS
SELECT s.id AS source_id, s.project_id, 'learning:' || k.session_id AS reference,
       'learning'::text AS label, s.title AS name,
       concat_ws(E'\n\n', array_to_string(k.topics, ', '), memory.session_learnings_text(k.learnings)) AS body
  FROM memory.session_knowledge k
  JOIN sessions.sessions s ON s.session_uuid = k.session_id
 WHERE cardinality(k.learnings) > 0 AND s.project_id IS NOT NULL;

CREATE VIEW memory.knowledge_body_mail_v AS
SELECT m.id AS source_id, COALESCE(th.project_id, fs.project_id) AS project_id,
       'mail:' || m.message_uuid AS reference, 'mail'::text AS label, fs.title AS name, m.body
  FROM mailbox.messages m
  JOIN sessions.sessions fs ON fs.id = m.from_session_id
  LEFT JOIN mailbox.threads th ON th.id = m.thread_id
 WHERE COALESCE(th.project_id, fs.project_id) IS NOT NULL;

CREATE VIEW memory.knowledge_body_session_v AS
SELECT ov.id AS source_id, ov.project_id, 'session:' || ov.session_uuid AS reference,
       'session'::text AS label, ov.title AS name,
       concat_ws(E'\n\n', ov.overview, 'Topics: ' || NULLIF(array_to_string(ok.topics, ', '), '')) AS body
  FROM sessions.sessions ov
  LEFT JOIN memory.session_knowledge ok ON ok.session_id = ov.session_uuid
 WHERE btrim(COALESCE(ov.overview, '')) <> '' AND ov.project_id IS NOT NULL;

INSERT INTO memory.knowledge_kinds (kind, body_view, settle_secs, built_in) VALUES
    ('artifact', 'memory.knowledge_body_artifact_v', 0, true),
    ('lesson', 'memory.knowledge_body_lesson_v', 0, true),
    ('note', 'memory.knowledge_body_note_v', 0, true),
    ('thread', 'memory.knowledge_body_thread_v', 600, true),
    ('thread_entry', 'memory.knowledge_body_thread_entry_v', 0, true),
    ('question_answer', 'memory.knowledge_body_question_answer_v', 0, true),
    ('unmet_ask', 'memory.knowledge_body_unmet_ask_v', 0, true),
    ('correction', 'memory.knowledge_body_correction_v', 0, true),
    ('learning', 'memory.knowledge_body_learning_v', 0, true),
    ('mail', 'memory.knowledge_body_mail_v', 0, true),
    ('session', 'memory.knowledge_body_session_v', 0, true);

INSERT INTO memory.knowledge_kind_watches
    (kind, watched_table, key_column, update_columns, via_table, via_match_column, via_key_column) VALUES
    ('artifact', 'memory.knowledge_documents', 'current_revision', '{current_revision,deleted_at,source_uuid}', NULL, NULL, NULL),
    ('lesson', 'memory.lessons', 'id', '{text,project_id,topic,kind_id}', NULL, NULL, NULL),
    ('note', 'workspace.session_notes', 'id', '{title,body,kind,session_id}', NULL, NULL, NULL),
    ('thread', 'mailbox.messages', 'thread_id', '{body,thread_id}', NULL, NULL, NULL),
    ('thread', 'mailbox.threads', 'id', '{subject,project_id}', NULL, NULL, NULL),
    ('thread_entry', 'work.thread_log', 'id', '{kind,summary,body}', NULL, NULL, NULL),
    ('question_answer', 'workspace.questions', 'id', '{question,answer,project_id}', NULL, NULL, NULL),
    ('unmet_ask', 'memory.turn_verdicts', 'operator_turn_id', '{mode,unmet_ask,rationale}', 'memory.turn_verdicts', 'operator_turn_id', 'id'),
    ('correction', 'memory.turn_verdicts', 'operator_turn_id', '{mode,reaction,rationale}', 'memory.turn_verdicts', 'operator_turn_id', 'id'),
    ('learning', 'memory.session_knowledge', 'session_id', '{learnings,topics}', 'sessions.sessions', 'session_uuid', 'id'),
    ('mail', 'mailbox.messages', 'id', '{body,thread_id,from_session_id}', NULL, NULL, NULL),
    ('session', 'sessions.sessions', 'id', '{overview,project_id}', NULL, NULL, NULL),
    ('session', 'memory.session_knowledge', 'session_id', '{topics}', 'sessions.sessions', 'session_uuid', 'id');

CREATE INDEX idx_knowledge_documents_current_revision ON memory.knowledge_documents (current_revision);

ALTER TABLE memory.knowledge_embedding_state ADD COLUMN item_id bigint;

INSERT INTO memory.knowledge_items (kind, source_id, needs_embedding, ready_at, attempts, last_error)
SELECT e.source_kind, e.source_id,
       e.completed_at IS NULL AND (e.attempts < 10 OR e.last_error LIKE '[transient]%'),
       CASE WHEN e.completed_at IS NULL THEN e.next_attempt_at ELSE now() END,
       CASE WHEN e.completed_at IS NULL THEN e.attempts ELSE 0 END,
       CASE WHEN e.completed_at IS NULL THEN e.last_error END
  FROM memory.knowledge_embedding_state e;

UPDATE memory.knowledge_items i SET needs_embedding = true, ready_at = now()
  FROM memory.knowledge_embedding_state e
  JOIN memory.knowledge_sources_with_sessions_v s ON s.source_kind = e.source_kind AND s.source_id = e.source_id
 WHERE e.source_kind = i.kind AND e.source_id = i.source_id AND e.completed_at IS NOT NULL
   AND (e.source_updated_at IS DISTINCT FROM s.source_updated_at
        OR e.source_seq IS DISTINCT FROM s.source_seq
        OR e.source_hash IS DISTINCT FROM s.source_hash);

DELETE FROM memory.knowledge_embedding_state WHERE completed_at IS NULL;

UPDATE memory.knowledge_embedding_state e SET item_id = i.id
  FROM memory.knowledge_items i WHERE i.kind = e.source_kind AND i.source_id = e.source_id;

UPDATE memory.knowledge_items i SET project_id = e.project_id, model_key = e.model_key
  FROM memory.knowledge_embedding_state e
 WHERE e.item_id = i.id
   AND (e.project_id IS NULL OR EXISTS (SELECT 1 FROM projects.projects p WHERE p.id = e.project_id));

CREATE OR REPLACE FUNCTION memory.fill_knowledge_embedding_scope() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.item_id IS NULL THEN
        INSERT INTO memory.knowledge_items AS i (kind, source_id, needs_embedding, model_key)
        VALUES (NEW.source_kind, NEW.source_id, NEW.completed_at IS NULL, NEW.model_key)
        ON CONFLICT (kind, source_id) DO UPDATE SET kind = EXCLUDED.kind
        RETURNING i.id INTO NEW.item_id;
    END IF;
    IF NEW.reference IS NULL THEN
        SELECT b.project_id, b.reference INTO NEW.project_id, NEW.reference
          FROM memory.knowledge_item_bodies(NEW.source_kind, ARRAY[NEW.source_id]) b
         LIMIT 1;
    END IF;
    RETURN NEW;
END
$$;

CREATE FUNCTION memory.requeue_unembedded_knowledge_item() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    UPDATE memory.knowledge_items i
       SET needs_embedding = true, ready_at = now(), changed_at = clock_timestamp()
      FROM old_rows o
     WHERE i.id = o.item_id AND NOT i.needs_embedding;
    RETURN NULL;
END
$$;

CREATE TRIGGER trg_knowledge_embedding_state_requeue AFTER DELETE ON memory.knowledge_embedding_state
    REFERENCING OLD TABLE AS old_rows FOR EACH STATEMENT
    EXECUTE FUNCTION memory.requeue_unembedded_knowledge_item();

DROP TRIGGER trg_knowledge_embedding_state_scope ON memory.knowledge_embedding_state;
CREATE TRIGGER trg_knowledge_embedding_state_scope BEFORE INSERT OR UPDATE OF source_kind, source_id, item_id
    ON memory.knowledge_embedding_state FOR EACH ROW EXECUTE FUNCTION memory.fill_knowledge_embedding_scope();

ALTER TABLE memory.knowledge_embedding_state
    DROP CONSTRAINT knowledge_embedding_state_source_kind_check,
    DROP CONSTRAINT knowledge_embedding_state_version_shape_check,
    ALTER COLUMN item_id SET NOT NULL,
    ADD CONSTRAINT knowledge_embedding_state_item_key UNIQUE (item_id),
    ADD CONSTRAINT knowledge_embedding_state_item_fkey FOREIGN KEY (item_id)
        REFERENCES memory.knowledge_items(id) ON DELETE CASCADE,
    ADD CONSTRAINT knowledge_embedding_state_kind_fkey FOREIGN KEY (source_kind)
        REFERENCES memory.knowledge_kinds(kind) ON DELETE CASCADE;

SELECT memory.reconcile_knowledge_items(k.kind) FROM memory.knowledge_kinds k;

UPDATE memory.knowledge_items i SET ready_at = t.last_message_at + interval '10 minutes'
  FROM mailbox.threads t
 WHERE i.kind = 'thread' AND i.source_id = t.id AND i.needs_embedding
   AND t.last_message_at > now() - interval '10 minutes';

SELECT memory.attach_knowledge_watch(w.id) FROM memory.knowledge_kind_watches w ORDER BY w.id;
