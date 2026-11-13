WITH incoming AS (
    SELECT d.document_uuid, x.* FROM jsonb_to_recordset($2) AS x(
        relative_path text, title text, body text, content_hash text,
        kind text, status text, metadata jsonb)
    JOIN memory.knowledge_documents d ON d.source_uuid = $1 AND d.relative_path = x.relative_path
    LEFT JOIN memory.knowledge_revisions r ON r.id = d.current_revision
    WHERE r.id IS NULL OR r.content_hash <> x.content_hash OR (r.metadata - 'source_git_commit' - 'source_worktree_dirty')
           <> (x.metadata - 'source_git_commit' - 'source_worktree_dirty')
       OR r.title <> x.title OR r.kind <> x.kind OR r.status <> x.status
), inserted AS (
    INSERT INTO memory.knowledge_revisions(document_uuid, title, body, content_hash, kind, status, metadata)
    SELECT document_uuid, title, body, content_hash, kind, status, metadata FROM incoming
    RETURNING document_uuid, id
)
UPDATE memory.knowledge_documents d SET current_revision = i.id
FROM inserted i WHERE d.document_uuid = i.document_uuid
