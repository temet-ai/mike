INSERT INTO runtime.capture_exchanges AS e
            (exchange_uuid, session_id, provider_thread_id, created_at, metadata,
             request_document_id, sent_document_id, response_document_id, status)
         VALUES ('dcc7fb83-fd31-43dd-a83c-64f5de1e8ff3'::uuid, 1, 'perf-thread', '2026-11-23T19:45:41.234368+00:00'::timestamptz, '{"wire":"updated"}'::jsonb, 1, 1, NULL, 'complete')
         ON CONFLICT (exchange_uuid) DO UPDATE SET
             session_id = COALESCE(EXCLUDED.session_id, e.session_id),
             provider_thread_id = COALESCE(EXCLUDED.provider_thread_id, e.provider_thread_id),
             metadata = EXCLUDED.metadata,
             response_document_id = COALESCE(EXCLUDED.response_document_id, e.response_document_id),
             status = EXCLUDED.status
         WHERE e.request_document_id = EXCLUDED.request_document_id
           AND e.sent_document_id = EXCLUDED.sent_document_id
           AND e.created_at = EXCLUDED.created_at
           AND (e.session_id IS NULL OR EXCLUDED.session_id IS NULL OR e.session_id = EXCLUDED.session_id)
           AND (e.provider_thread_id IS NULL OR EXCLUDED.provider_thread_id IS NULL OR e.provider_thread_id = EXCLUDED.provider_thread_id)
           AND (e.response_document_id IS NULL OR EXCLUDED.response_document_id IS NULL OR e.response_document_id = EXCLUDED.response_document_id)
           AND (e.status = 'pending' OR e.status = EXCLUDED.status)
         RETURNING WITH (OLD AS before, NEW AS after) after.session_id,
             (before.id IS NULL OR
              ROW(before.session_id, before.provider_thread_id, before.metadata, before.response_document_id, before.status)
              IS DISTINCT FROM
              ROW(after.session_id, after.provider_thread_id, after.metadata, after.response_document_id, after.status)) AS "changed!";
