SELECT count(*) AS "asked!",
       count(*) FILTER (WHERE q.state = 'answered') AS "answered!"
  FROM workspace.questions q
 WHERE q.run_id = $1
