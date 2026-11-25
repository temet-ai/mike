-- Code findings are no longer pushed to idle sessions. The toggle's row goes; an absent key is a no-op.

SET LOCAL lock_timeout = '10s';

DELETE FROM config.feature_toggles
WHERE key = 'code_findings_push';
