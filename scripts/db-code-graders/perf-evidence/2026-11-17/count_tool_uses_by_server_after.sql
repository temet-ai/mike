SELECT CASE WHEN tu.tool_name LIKE 'mcp\_\_%' THEN split_part(tu.tool_name, '__', 2) ELSE 'built-in' END AS key, count(*) AS count FROM tools.uses tu WHERE TRUE GROUP BY 1 ORDER BY 2 DESC, 1
