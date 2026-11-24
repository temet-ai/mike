-- The file-read and shell-output compressors are removed. Their two toggle rows go.
--
-- config.feature_toggles has had no CHECK on key since 20261112160000, so no constraint needs
-- rebuilding. The row delete is recorded in config.change_log by trg_feature_toggles_change_log;
-- the history stays. An absent key is a no-op.

SET LOCAL lock_timeout = '10s';

DELETE FROM config.feature_toggles
WHERE key IN ('file_read_compression', 'shell_output_compression');
