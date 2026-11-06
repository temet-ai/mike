INSERT INTO runtime.threads
       (session_id, namespace_id, provider_thread_id, provider_session_id, owning_seat_id,
        native_parent_namespace_id, native_parent_thread_id, fork_namespace_id,
        fork_thread_id, identity)
SELECT session.id, $2::uuid, $3::text, $4::text, NULL, $5::uuid, $6::text, $7::uuid, $8::text, $9::jsonb
  FROM sessions.sessions session
 WHERE session.session_uuid = $1::uuid
ON CONFLICT DO NOTHING
RETURNING session_id
