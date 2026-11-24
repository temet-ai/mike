SELECT DISTINCT t.thread_id, t.runtime_home
  FROM runtime.notification_targets t
  JOIN sessions.sessions s ON s.id = t.session_id
  LEFT JOIN runtime.session_facts f ON f.session_id = t.session_id
  LEFT JOIN runtime.threads thread ON thread.session_id = s.id AND thread.provider_thread_id = t.thread_id
  LEFT JOIN runtime.namespaces n ON n.id = thread.namespace_id
       AND n.runtime_kind = 'codex' AND n.durable_key = 'codex-home:' || t.runtime_home
  LEFT JOIN sessions.sessions owner ON owner.id = COALESCE(thread.owning_seat_id, thread.session_id)
       AND n.id IS NOT NULL
  LEFT JOIN launch.launches l ON l.session_uuid = owner.session_uuid
 WHERE s.session_uuid = $1
   AND (f.control = 'hook_boundary' OR (n.id IS NOT NULL AND (
       l.id IS NULL OR (l.closed_at IS NULL
           AND l.identity->>'runtime' = 'codex' AND l.identity->>'runtime_home' = t.runtime_home)
   )))
   AND f.lifecycle IS DISTINCT FROM 'ended' AND f.lifecycle IS DISTINCT FROM 'failed'
   AND t.verified_at > now() - interval '7 days'
