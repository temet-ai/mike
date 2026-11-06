WITH src AS (SELECT '664e249b-93b2-4896-be25-0b43aab4895b'::uuid AS source_uuid),
scan AS (
  SELECT jsonb_agg(jsonb_build_object('relative_path', d.relative_path, 'title', r.title, 'body', r.body,
         'content_hash', r.content_hash, 'kind', r.kind, 'status', r.status,
         'metadata', r.metadata || '{"source_git_commit":"0000000000000000000000000000000000000000","source_worktree_dirty":false}'::jsonb)) AS docs
  FROM memory.knowledge_documents d JOIN memory.knowledge_revisions r ON r.id = d.current_revision
  WHERE d.source_uuid = '664e249b-93b2-4896-be25-0b43aab4895b' AND d.deleted_at IS NULL
), incoming AS (
    SELECT d.document_uuid, x.* FROM scan, jsonb_to_recordset(scan.docs) AS x(
        relative_path text, title text, body text, content_hash text,
        kind text, status text, metadata jsonb)
    JOIN memory.knowledge_documents d ON d.source_uuid = '664e249b-93b2-4896-be25-0b43aab4895b' AND d.relative_path = x.relative_path
    LEFT JOIN memory.knowledge_revisions r ON r.id = d.current_revision
    WHERE r.id IS NULL OR r.content_hash <> x.content_hash OR (r.metadata - 'source_git_commit' - 'source_worktree_dirty') <> (x.metadata - 'source_git_commit' - 'source_worktree_dirty')
       OR r.title <> x.title OR r.kind <> x.kind OR r.status <> x.status
), inserted AS (
    INSERT INTO memory.knowledge_revisions(document_uuid, title, body, content_hash, kind, status, metadata)
    SELECT document_uuid, title, body, content_hash, kind, status, metadata FROM incoming
    RETURNING document_uuid, id
)
UPDATE memory.knowledge_documents d SET current_revision = i.id
FROM inserted i WHERE d.document_uuid = i.document_uuid
