CREATE VIEW memory.knowledge_embedding_sources_v AS
SELECT 'artifact:' || r.id AS reference, loc.project_id, r.content_hash AS content_hash,
       r.title || E'\n\n' || r.body AS body
FROM memory.knowledge_documents d
JOIN memory.knowledge_revisions r ON r.id = d.current_revision
JOIN memory.knowledge_sources src ON src.source_uuid = d.source_uuid
JOIN projects.locations loc ON loc.location_uuid = src.location_uuid
WHERE d.deleted_at IS NULL
UNION ALL
SELECT 'lesson:' || l.uuid, l.project_id, md5(l.text), l.text
FROM memory.lessons l WHERE l.project_id IS NOT NULL
UNION ALL
SELECT 'note:' || n.note_uuid, s.project_id, md5(n.title || E'\n\n' || n.body),
       n.title || E'\n\n' || n.body
FROM workspace.session_notes n JOIN sessions.sessions s ON s.id = n.session_id;

CREATE TABLE memory.knowledge_embedding_state (
    reference text PRIMARY KEY,
    content_hash text NOT NULL,
    model_key text NOT NULL,
    completed_at timestamptz,
    attempts integer NOT NULL DEFAULT 0,
    next_attempt_at timestamptz NOT NULL DEFAULT now(),
    last_error text
);
CREATE TABLE memory.knowledge_embedding_chunks (
    reference text NOT NULL REFERENCES memory.knowledge_embedding_state(reference) ON DELETE CASCADE,
    ordinal integer NOT NULL,
    body text NOT NULL,
    embedding public.vector(768) NOT NULL,
    PRIMARY KEY(reference, ordinal)
);
