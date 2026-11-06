-- no-transaction
-- 32 MB, 0 scans; requests.tools is only read by request_id.
DROP INDEX CONCURRENTLY IF EXISTS requests.idx_request_tools_name;
