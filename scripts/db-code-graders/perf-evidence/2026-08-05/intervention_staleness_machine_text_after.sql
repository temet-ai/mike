SELECT i.expires_at, (sub.id IS NOT NULL) AS "superseded"
     FROM steering.interventions i
     LEFT JOIN LATERAL (
         SELECT r.id
           FROM requests.requests r
          WHERE r.session_id = i.session_id
            AND r.id > i.trigger_request_id
            AND r.human_prompt <> ''
            AND r.turn_origin_id IN (1, 6)
            AND r.human_prompt NOT LIKE '<channel %'
            AND r.human_prompt NOT LIKE '%<task-notification>%'
            AND r.human_prompt NOT LIKE '%<teammate-message%'
            AND r.human_prompt NOT LIKE '%<agent-message from=%'
            AND r.human_prompt NOT LIKE '%Stop hook feedback:%'
            AND r.human_prompt NOT LIKE '%[SYSTEM NOTIFICATION - NOT USER INPUT]%'
            AND r.human_prompt NOT LIKE 'Tool loaded.%'
            AND r.human_prompt NOT LIKE '%<<autonomous-loop-dynamic>>%'
            AND r.human_prompt NOT LIKE '%<<autonomous-loop>>%'
            AND r.human_prompt NOT LIKE '%Fleet coordinator cycle wake%'
            AND r.human_prompt NOT LIKE '%Cross-session message from%'
            AND r.human_prompt NOT LIKE '%The user stepped away and is coming back%'
            AND r.human_prompt NOT LIKE '%Steering note from the operator''s proxy:%'
          LIMIT 1
     ) sub ON true
    WHERE i.uuid = 'bb4ffc05-61cc-4436-b5da-34ff16312e1d';
