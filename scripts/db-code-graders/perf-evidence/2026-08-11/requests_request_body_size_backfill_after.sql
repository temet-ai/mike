UPDATE requests.requests
   SET request_body_size = COALESCE(octet_length(request_body), 0)
 WHERE request_body_size IS NULL;
