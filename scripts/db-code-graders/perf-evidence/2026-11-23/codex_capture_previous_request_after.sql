SELECT e.request_document_id FROM runtime.capture_exchanges e
JOIN sessions.sessions s ON s.id = e.session_id
JOIN runtime.capture_documents d ON d.id = e.request_document_id
WHERE s.session_uuid = 'd7d54e5d-647a-4c43-9b79-e8590f2f3100'::uuid
  AND e.provider_thread_id = 'perf-thread'
  AND d.finished AND d.complete AND d.byte_size > 0
ORDER BY e.created_at DESC, e.id DESC LIMIT 1;
