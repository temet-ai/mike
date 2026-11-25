-- Nothing reads the codex_diagnostic_capture toggle. Its row goes; an absent key is a no-op.

SET LOCAL lock_timeout = '10s';

DELETE FROM config.feature_toggles
WHERE key = 'codex_diagnostic_capture';
