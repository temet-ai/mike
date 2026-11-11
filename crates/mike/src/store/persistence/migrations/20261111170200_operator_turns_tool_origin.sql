INSERT INTO prompt_kinds.kinds (code) VALUES ('tool-prompt')
ON CONFLICT (code) DO NOTHING;

WITH tool_turns AS (
    UPDATE memory.operator_turns t
       SET opened_by = 'machine',
           judge_queued_at = NULL
     WHERE (t.opened_by IS NULL OR t.opened_by = 'operator')
       AND t.user_prompt ~ '^\s*(A session-scoped Stop hook is now active with condition:|The [A-Za-z0-9_.-]+ plugin sent a message:|Perform a web search for the query:|You score how well a stand-in predicted|Coordinator wake for fleet )'
 RETURNING t.id
)
UPDATE memory.turn_facts f
   SET prompt_kind = 'background-event',
       prompt_rule = 'a'
  FROM tool_turns
 WHERE f.operator_turn_id = tool_turns.id
   AND f.prompt_kind <> 'background-event';

UPDATE prompt_kinds.classifications c
   SET kind_id = tool.id,
       rule = 'a',
       classifier_version = 'genuine-v3'
  FROM prompt_kinds.kinds tool,
       prompt_kinds.kinds operator,
       requests.requests r
 WHERE tool.code = 'tool-prompt'
   AND operator.code = 'operator'
   AND c.kind_id = operator.id
   AND r.id = c.request_id
   AND r.human_prompt ~ '^\s*(A session-scoped Stop hook is now active with condition:|The [A-Za-z0-9_.-]+ plugin sent a message:|Perform a web search for the query:|You score how well a stand-in predicted|Coordinator wake for fleet )';
