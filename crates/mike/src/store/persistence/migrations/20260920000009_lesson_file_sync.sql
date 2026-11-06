ALTER TABLE memory.lessons ADD COLUMN portable_origin jsonb NOT NULL DEFAULT '{}';
CREATE UNIQUE INDEX lessons_portable_uuid ON memory.lessons(uuid);
CREATE TABLE memory.lesson_file_changes (
    lesson_uuid uuid PRIMARY KEY,
    project_id bigint NOT NULL REFERENCES projects.projects(id),
    version bigint NOT NULL DEFAULT 1,
    payload jsonb NOT NULL,
    changed_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX lesson_file_changes_project ON memory.lesson_file_changes(project_id, lesson_uuid);
CREATE TABLE memory.lesson_file_receipts (
    location_uuid uuid NOT NULL REFERENCES projects.locations(location_uuid),
    lesson_uuid uuid NOT NULL REFERENCES memory.lesson_file_changes(lesson_uuid),
    version bigint NOT NULL,
    written_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY(location_uuid, lesson_uuid)
);
CREATE VIEW memory.portable_lesson_v AS
WITH relations AS (
    SELECT r.from_lesson_id, jsonb_agg(jsonb_build_object(
        'target_uuid', target.uuid, 'kind', k.code, 'status', st.code,
        'method', m.code, 'evidence', r.evidence_json,
        'proposer_model', r.proposer_model, 'proposer_rationale', r.proposer_rationale,
        'adjudicator_model', r.adjudicator_model, 'adjudicator_agrees', r.adjudicator_agrees,
        'adjudicator_rationale', r.adjudicator_rationale, 'decided_by', r.decided_by,
        'detector_version', r.detector_version) ORDER BY r.id) AS items
    FROM memory.lesson_relations r
    JOIN memory.lessons target ON target.id = r.to_lesson_id
    JOIN memory.lesson_relation_kinds k ON k.id = r.kind_id
    JOIN memory.lesson_relation_statuses st ON st.id = r.status_id
    JOIN memory.lesson_relation_methods m ON m.id = r.method_id
    GROUP BY r.from_lesson_id
)
SELECT l.id, l.uuid, l.project_id, jsonb_build_object(
    'lesson_uuid', l.uuid, 'project_uuid', p.project_uuid, 'body', l.text,
    'kind', k.code, 'stage', st.code, 'topic', l.topic, 'source', src.code,
    'created_at', l.created_at, 'expires_at', l.expires_at, 'archived_at', l.archived_at,
    'archived_reason', ar.code, 'agent_name', a.name, 'agent_uuid', a.uuid,
    'session_uuid', COALESCE(to_jsonb(s.session_uuid), l.portable_origin->'session_uuid'),
    'source_turn_id', COALESCE(to_jsonb(l.source_turn_id), l.portable_origin->'source_turn_id'),
    'duplicate_of_uuid', COALESCE(to_jsonb(dup.uuid), l.portable_origin->'duplicate_of_uuid'), 'relations', COALESCE(r.items, '[]'::jsonb),
    'deleted', false) AS payload
FROM memory.lessons l
JOIN projects.projects p ON p.id = l.project_id
JOIN memory.lesson_kinds k ON k.id = l.kind_id
JOIN memory.lesson_stages st ON st.id = l.stage_id
JOIN memory.lesson_sources src ON src.id = l.source_id
LEFT JOIN memory.lesson_archived_reasons ar ON ar.id = l.archived_reason_id
LEFT JOIN agents.agents a ON a.id = l.agent_id
LEFT JOIN sessions.sessions s ON s.id = l.session_id
LEFT JOIN memory.lessons dup ON dup.id = l.duplicate_of_id
LEFT JOIN relations r ON r.from_lesson_id = l.id;

CREATE FUNCTION memory.queue_lesson_file() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        IF OLD.project_id IS NOT NULL THEN
            INSERT INTO memory.lesson_file_changes(lesson_uuid, project_id, payload)
            VALUES (OLD.uuid, OLD.project_id, jsonb_build_object('lesson_uuid', OLD.uuid, 'deleted', true))
            ON CONFLICT(lesson_uuid) DO UPDATE SET version = memory.lesson_file_changes.version + 1,
                payload = memory.lesson_file_changes.payload || jsonb_build_object('deleted', true), changed_at = now();
        END IF;
        RETURN OLD;
    END IF;
    INSERT INTO memory.lesson_file_changes(lesson_uuid, project_id, payload)
        SELECT uuid, project_id, payload FROM memory.portable_lesson_v WHERE id = NEW.id
    ON CONFLICT(lesson_uuid) DO UPDATE SET project_id = EXCLUDED.project_id,
        version = memory.lesson_file_changes.version + 1,
        payload = EXCLUDED.payload || jsonb_build_object(
            'session_uuid', COALESCE(NULLIF(EXCLUDED.payload->'session_uuid', 'null'::jsonb), memory.lesson_file_changes.payload->'session_uuid'),
            'source_turn_id', COALESCE(NULLIF(EXCLUDED.payload->'source_turn_id', 'null'::jsonb), memory.lesson_file_changes.payload->'source_turn_id')),
        changed_at = now()
    WHERE memory.lesson_file_changes.payload IS DISTINCT FROM EXCLUDED.payload
       OR memory.lesson_file_changes.project_id <> EXCLUDED.project_id;
    RETURN NEW;
END $$;
CREATE TRIGGER lesson_file_queue AFTER INSERT OR UPDATE OR DELETE ON memory.lessons
    FOR EACH ROW EXECUTE FUNCTION memory.queue_lesson_file();

CREATE FUNCTION memory.queue_lesson_relation_file() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE target_id bigint;
BEGIN
    IF TG_OP = 'DELETE' THEN target_id := OLD.from_lesson_id;
    ELSE target_id := NEW.from_lesson_id; END IF;
    INSERT INTO memory.lesson_file_changes(lesson_uuid, project_id, payload)
        SELECT uuid, project_id, payload FROM memory.portable_lesson_v WHERE id = target_id
    ON CONFLICT(lesson_uuid) DO UPDATE SET version = memory.lesson_file_changes.version + 1,
        payload = EXCLUDED.payload, changed_at = now()
    WHERE memory.lesson_file_changes.payload IS DISTINCT FROM EXCLUDED.payload;
    RETURN NULL;
END $$;
CREATE TRIGGER lesson_relation_file_queue AFTER INSERT OR UPDATE OR DELETE ON memory.lesson_relations
    FOR EACH ROW EXECUTE FUNCTION memory.queue_lesson_relation_file();
INSERT INTO memory.lesson_file_changes(lesson_uuid, project_id, payload)
    SELECT uuid, project_id, payload FROM memory.portable_lesson_v;
COMMENT ON TABLE memory.lesson_file_changes IS 'Transactional coalescing outbox of portable lesson revisions, including deletion tombstones. Filesystem delivery is acknowledged per location and revision.';
