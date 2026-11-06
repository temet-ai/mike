INSERT INTO responses.stream_blocks
     (request_id, session_id, block_type_id, seq_ord, content,
      signature, wire_tool_use_id, content_sha256, byte_size)
     SELECT 733608, r.session_id, sbt.id, 999, 'measured text', NULL, NULL, 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 13
     FROM requests.requests r, responses.stream_block_types sbt
     WHERE r.id = 733608
       AND sbt.code = 'text'
     ON CONFLICT (request_id, seq_ord) DO NOTHING;
