SELECT event.id AS "event_id!",
       event.namespace_id AS "namespace_id!",
       event.ingestion_id AS "ingestion_id!",
       event.envelope::text AS "envelope!",
       event.raw_payload AS "raw_payload!",
       event.parsed_payload::text AS "parsed_payload?",
       context.capture_sequence AS "capture_sequence?",
       context.context_version AS "context_version?",
       context.connection_epoch AS "connection_epoch?",
       context.frame_ordinal AS "frame_ordinal?",
       context.transport_gap AS "transport_gap?",
       context.context_document::text AS "context_document?"
  FROM runtime.codex_receipt_context context
  JOIN runtime.events event
    ON event.id = context.event_id AND event.namespace_id = context.namespace_id
 WHERE context.namespace_id = $1
   AND context.capture_sequence > $3
   AND NOT EXISTS (SELECT 1 FROM runtime.codex_projection_applications application
                    WHERE application.generation_id = $2 AND application.event_id = event.id)
 ORDER BY context.capture_sequence
 LIMIT 1
