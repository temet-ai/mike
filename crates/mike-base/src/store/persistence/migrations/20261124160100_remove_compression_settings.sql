-- The file-read and shell-output compressors are removed. Their live settings rows go:
-- file_read_compression.*, shell_output_compression.*, compress.* and compressor_loop.*.
--
-- The delete is recorded in config.change_log by trg_settings_change_log; the history stays.
-- compression_dedup.enabled (read_dedup) is a different key and stays. config.machine_settings
-- holds no such rows (checked 2026-11-24), so it is not touched. An absent key is a no-op.

SET LOCAL lock_timeout = '10s';

DELETE FROM config.settings
WHERE key LIKE 'file\_read\_compression.%'
   OR key LIKE 'shell\_output\_compression.%'
   OR key LIKE 'compress.%'
   OR key LIKE 'compressor\_loop.%';
