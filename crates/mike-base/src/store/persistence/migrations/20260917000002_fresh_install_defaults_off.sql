UPDATE config.feature_toggles
   SET enabled = false, operator_set = false
 WHERE key IN (
     'reminders',
     'recall_injector',
     'weasel_intercept',
     'steerer',
     'judge_worker',
     'session_judge',
     'reminder_on_token',
     'mailbox_redelivery',
     'agent_lessons',
     'file_read_compression',
     'self_identity_reminder'
 )
   AND NOT EXISTS (SELECT 1 FROM requests.requests LIMIT 1);
