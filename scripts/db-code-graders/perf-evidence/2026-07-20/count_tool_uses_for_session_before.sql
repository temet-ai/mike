SELECT COUNT(*)
  FROM tools.uses tu
  JOIN requests.requests r ON r.id = tu.request_id
 WHERE r.session_id = 100749;
