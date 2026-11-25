-- Nothing reads the context_focus_question toggle or context_focus_question.mode. Their rows go;
-- the field in captured tool calls stays, as captured.

SET LOCAL lock_timeout = '10s';

DELETE FROM config.feature_toggles
WHERE key = 'context_focus_question';

DELETE FROM config.settings
WHERE key = 'context_focus_question.mode';
