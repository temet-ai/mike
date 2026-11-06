SELECT
       ce.id          AS "id!",
       s.session_uuid AS "session_id?",
       ce.actor_label AS "actor_label!",
       ek.code        AS "kind!",
       ce.subject     AS "subject!",
       ce.detail      AS "detail!",
       ce.created_at  AS "created_at!"
   FROM workspace.coordination_events ce
   JOIN workspace.event_kinds ek ON ek.id = ce.kind_id
   LEFT JOIN sessions.sessions s ON s.id = ce.session_id
  WHERE ce.project_id = 192756
  ORDER BY ce.id DESC
  OFFSET 0 LIMIT 100
