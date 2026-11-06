SELECT
       s.id          AS "id!",
       s.uuid        AS "uuid!",
       s.name        AS "name!",
       s.display_name AS "display_name!",
       s.description AS "description!",
       s.agent_id    AS "agent_id!",
       a.code        AS "authority!",
       m.code        AS "mode!",
       s.model       AS "model!",
       s.priority    AS "priority!",
       s.enabled     AS "enabled!",
       s.config      AS "config!",
       s.prompt,
       s.kind         AS "kind!",
       s.trigger_point AS "trigger_point!",
       s.invocation   AS "invocation!",
       s.pattern,
       s.auto_enrol   AS "auto_enrol!",
       s.created_at  AS "created_at!",
       s.updated_at  AS "updated_at!"
   FROM steering.steerers s
   JOIN steering.authorities a ON a.id = s.authority_id
   JOIN steering.modes       m ON m.id = s.default_mode_id
  WHERE s.deleted_at IS NULL
   ORDER BY s.priority ASC, s.id ASC
