SELECT r.id AS "id!", r.uuid AS "uuid!", k.code AS "kind!", r.value AS "value!",
       a.code AS "action!", r.text, r.trigger_token, r.inject_text, r.enabled AS "enabled!",
       s.code AS "source!", r.updated_at AS "updated_at!",
       r.project_path, r.sort_order AS "sort_order!",
       r.style_override AS "style_override!", r.title
   FROM rules.rules r
   JOIN rules.kinds   k ON k.id = r.kind_id
   JOIN rules.actions a ON a.id = r.action_id
   JOIN rules.sources s ON s.id = r.source_id
   ORDER BY r.sort_order ASC, r.id ASC
