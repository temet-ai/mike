UPDATE tools.uses u
SET session_id = req.session_id
FROM requests.requests req
WHERE req.id = u.request_id
  AND u.session_id IS NULL;
