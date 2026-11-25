-- Nothing reads the session_end_shadow toggle or its five session_end_shadow.* settings. Their rows go; stored shadow verdicts in session_judges.verdicts stay.

SET LOCAL lock_timeout = '10s';

DELETE FROM config.feature_toggles
WHERE key = 'session_end_shadow';

DELETE FROM config.settings
WHERE starts_with(key, 'session_end_shadow.');
