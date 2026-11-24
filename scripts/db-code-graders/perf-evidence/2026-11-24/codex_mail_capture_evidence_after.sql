SELECT c.exchange_uuid, d.delivered_at
                   FROM mailbox.capture_deliveries d
                   JOIN runtime.capture_exchanges c ON c.id = d.capture_id
                   JOIN mailbox.messages m ON m.id = d.message_id
                   JOIN sessions.sessions s ON s.id = m.to_session_id
                  WHERE s.session_uuid = '1b09492d-dca1-4228-91bc-7e5418fa5301'::uuid AND m.message_uuid = '6636110c-edc2-4a9a-b262-ecd1485d747b'::uuid
                    AND c.session_id = s.id
                  ORDER BY d.id