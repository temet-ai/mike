INSERT INTO responses.stream_blocks
    (request_id, session_id, block_type_id, seq_ord, content,
     signature, wire_tool_use_id, content_sha256, byte_size)
 SELECT 279614, 769987, sbt.id, u.seq_ord, u.content,
        u.signature, u.wire_tool_use_id, u.content_sha256, u.byte_size
 FROM UNNEST(
        ARRAY[999999]::int[],
        ARRAY['text']::text[],
        ARRAY['perfcheck retry content']::text[],
        ARRAY[NULL]::text[],
        ARRAY[NULL]::text[],
        ARRAY[repeat('0', 64)]::text[],
        ARRAY[octet_length('perfcheck retry content')]::int[]
      )
      AS u(seq_ord, block_type, content, signature, wire_tool_use_id, content_sha256, byte_size)
 JOIN responses.stream_block_types sbt ON sbt.code = u.block_type
 ON CONFLICT (request_id, seq_ord) DO NOTHING;
