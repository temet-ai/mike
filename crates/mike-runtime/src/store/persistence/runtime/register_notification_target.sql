INSERT INTO runtime.notification_targets (session_id, thread_id, runtime_home)
SELECT DISTINCT s.id, $2::text, $3::text
  FROM sessions.sessions s
  LEFT JOIN runtime.session_facts f ON f.session_id = s.id
  LEFT JOIN runtime.threads thread ON thread.session_id = s.id AND thread.provider_thread_id = $2
  LEFT JOIN runtime.namespaces n ON n.id = thread.namespace_id
       AND n.runtime_kind = 'codex' AND n.durable_key = 'codex-home:' || $3
  LEFT JOIN sessions.sessions owner ON owner.id = COALESCE(thread.owning_seat_id, thread.session_id)
       AND n.id IS NOT NULL
  LEFT JOIN launch.launches l ON l.session_uuid = owner.session_uuid
 WHERE s.session_uuid = $1
   AND (f.control = 'hook_boundary' OR (n.id IS NOT NULL AND (
       l.id IS NULL OR (l.closed_at IS NULL
           AND l.identity->>'runtime' = 'codex' AND l.identity->>'runtime_home' = $3)
   )))
ON CONFLICT (session_id) DO UPDATE SET thread_id = EXCLUDED.thread_id,
    runtime_home = EXCLUDED.runtime_home, verified_at = now()
