INSERT INTO runtime.threads
       (session_id, namespace_id, provider_thread_id, provider_session_id, owning_seat_id,
        native_parent_namespace_id, native_parent_thread_id, fork_namespace_id,
        fork_thread_id, identity)
SELECT session.id, $2::uuid, $3::text, $4::text, owner.id, $6::uuid, $7::text, $8::uuid, $9::text, $10::jsonb
  FROM sessions.sessions session
  JOIN sessions.sessions owner ON owner.session_uuid = $5::uuid
 WHERE session.session_uuid = $1::uuid
ON CONFLICT DO NOTHING
RETURNING session_id
