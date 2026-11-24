WITH recorded AS (
    INSERT INTO mailbox.capture_deliveries AS delivery (message_id, capture_id)
    SELECT m.id, c.id
      FROM runtime.capture_exchanges c
      JOIN sessions.sessions s ON s.id = c.session_id
      JOIN runtime.capture_documents original ON original.id = c.request_document_id
      JOIN runtime.capture_documents sent ON sent.id = c.sent_document_id
      JOIN mailbox.messages m ON m.to_session_id = s.id
     WHERE s.session_uuid = '1b09492d-dca1-4228-91bc-7e5418fa5301'::uuid AND c.exchange_uuid = 'dd390615-73fd-4d86-81ed-b7351e20e77c'::uuid
       AND m.message_uuid = ANY(ARRAY['9327a09e-de1c-4b7c-9e7f-5d2ef62a4141'::uuid])
       AND original.id <> sent.id
       AND original.finished AND original.complete
       AND sent.finished AND sent.complete
    ON CONFLICT (message_id, capture_id) DO UPDATE
        SET delivered_at = delivery.delivered_at
    RETURNING WITH (OLD AS before, NEW AS after)
        after.message_id, after.delivered_at, (before.id IS NULL) AS inserted
), stamped AS (
    UPDATE mailbox.messages m
       SET claimed_at = COALESCE(m.claimed_at, r.delivered_at),
           sent_at = COALESCE(m.sent_at, r.delivered_at),
           delivery_attempts = m.delivery_attempts + CASE WHEN m.sent_at IS NULL THEN 1 ELSE 0 END
      FROM recorded r
     WHERE r.message_id = m.id AND r.inserted
    RETURNING m.id
)
SELECT m.message_uuid AS "message_uuid!"
  FROM recorded r JOIN mailbox.messages m ON m.id = r.message_id
 ORDER BY m.id
