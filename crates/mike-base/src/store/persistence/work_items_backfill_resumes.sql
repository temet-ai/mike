SELECT u.tool_name, res.content, res.request_id
  FROM tools.results res
  JOIN tools.uses u ON u.id = res.tool_use_id
 WHERE res.session_id = $1
   AND u.tool_name = 'SendMessage'
   AND res.content::text LIKE '%resumedAgentId%'
