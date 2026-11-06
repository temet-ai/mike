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
  FROM runtime.events event
  LEFT JOIN runtime.codex_receipt_context context
    ON context.event_id = event.id AND context.namespace_id = event.namespace_id
 WHERE event.namespace_id = $1 AND event.id > $2
 ORDER BY event.id
 LIMIT $3
