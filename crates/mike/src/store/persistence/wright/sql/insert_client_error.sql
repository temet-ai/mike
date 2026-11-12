INSERT INTO wright.client_errors (route, kind, message, detail)
VALUES ($1, $2, $3, $4)
RETURNING id
