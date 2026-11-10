SELECT DISTINCT tu.request_id
FROM tools.uses tu
WHERE tu.request_id = ANY(ARRAY[760265,760266,760269,760271,760272,760275,760276,760280,760282,760284,760286,760289,760295,760297,760300,760306,760308,760309,760310,760311,760312,760315,760317,760318,760321,760322,760343,760346,760350,760351,760352,760353,760357,760358,760359,760360,760361,760362,760363,760364,760365,760369,760370,760371,760374,760377,760379,760380,760381,760382]::bigint[])
  AND EXISTS (
        SELECT 1
        FROM unnest(ARRAY['find_text','search_session_content','search_project_knowledge','recall_similar_turns','recall_project_context']::text[]) AS suffix(name)
        WHERE right(tu.tool_name, length(suffix.name)) = suffix.name
      )
  AND (CASE WHEN true THEN strpos(tu.input::text, 'b013f3306')
            ELSE strpos(lower(tu.input::text), lower('b013f3306')) END) > 0
