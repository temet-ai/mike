SELECT b.content FROM responses.stream_blocks b JOIN responses.stream_block_types t ON t.id = b.block_type_id WHERE b.request_id = 447654 AND t.code = 'text' ORDER BY b.seq_ord ASC
