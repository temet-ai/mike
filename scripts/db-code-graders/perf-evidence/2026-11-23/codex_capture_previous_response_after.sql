SELECT d.id FROM runtime.capture_exchanges e
JOIN sessions.sessions s ON s.id = e.session_id
JOIN runtime.capture_documents d ON d.id = e.response_document_id
WHERE s.session_uuid = 'b0b5b43d-37ee-41e0-8820-e1b7f3063778'::uuid
  AND e.provider_thread_id = 'response-perf-thread'
  AND d.finished AND d.byte_size > 0
ORDER BY e.created_at DESC, e.id DESC LIMIT 1;
