WITH RECURSIVE chain AS (
             SELECT r.id, r.predecessor_request_id, r.predecessor_rule_id, 0 AS depth
               FROM requests.requests r
              WHERE r.id = 855615
             UNION ALL
             SELECT p.id, p.predecessor_request_id, p.predecessor_rule_id, c.depth + 1
               FROM chain c
               JOIN requests.requests p ON p.id = c.predecessor_request_id
              WHERE c.predecessor_rule_id = (SELECT pr.id FROM requests.predecessor_rules pr WHERE pr.code = 'thread_link')
                AND c.depth < 1024
         ),
         bridged AS (
             SELECT DISTINCT ON (m.message_sha256)
                    m.message_sha256, b.message_id, c.depth, b.ord
               FROM chain c
               JOIN requests.messages b ON b.request_id = c.id
               JOIN messages.messages m ON m.id = b.message_id
              ORDER BY m.message_sha256, c.depth DESC, b.ord
         )
         SELECT (row_number() OVER (ORDER BY depth DESC, ord) - 1) AS seq,
                message_sha256, message_id
           FROM bridged
          ORDER BY seq;
