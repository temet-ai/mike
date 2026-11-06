SELECT r.id AS "rule_id!", k.code AS "kind!", r.value AS "label!",
       r.trigger_token AS "tag", r.enabled AS "enabled!",
       COUNT(DISTINCT sp.request_id) AS "fired_turns!"
  FROM requests.system_prompt_firing_rules f
  JOIN requests.system_prompts sp ON sp.id = f.request_system_prompt_id
  JOIN requests.requests req ON req.id = sp.request_id
  JOIN sessions.sessions s ON s.id = req.session_id
  JOIN rules.rules r ON r.id = f.filter_rule_id
  JOIN rules.kinds k ON k.id = r.kind_id
 WHERE s.session_uuid = '80c8b55c-59fc-47fe-afa8-3782a0c47231'
 GROUP BY r.id, k.code, r.value, r.trigger_token, r.enabled
 ORDER BY r.id ASC;
