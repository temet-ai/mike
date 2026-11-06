-- `requests.deltas.body`/`byte_size`/`role_id` mirrored `messages.messages`
-- (reachable via the row's own `message_id` FK) byte-for-byte on every row
-- of the table: 10,989,498 of 10,989,498 verified, zero mismatches, zero
-- rows whose `message_id` had no matching message. `body` alone accounted for
-- 98.6% of the table's row content; the table sits at 7,571 MB heap + 37 GB
-- TOAST. The write path (`messages::write_request_deltas`) and read path
-- (`messages::fetch_request_deltas_by_id`) now source all three fields by
-- JOIN on `message_id` instead of storing a duplicate copy.
--
-- Metadata-only: DROP COLUMN does not rewrite the table, so this runs
-- instantly regardless of table size and needs no VACUUM FULL. Postgres
-- auto-drops the single-column index (idx_request_deltas_role) and CHECK
-- constraint (request_deltas_byte_size_nonneg) that depend solely on the
-- dropped columns -- no explicit DROP INDEX/CONSTRAINT needed.
ALTER TABLE requests.deltas
    DROP COLUMN IF EXISTS body,
    DROP COLUMN IF EXISTS byte_size,
    DROP COLUMN IF EXISTS role_id;
