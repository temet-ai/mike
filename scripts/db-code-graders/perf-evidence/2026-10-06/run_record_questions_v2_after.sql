SELECT count(*) AS asked,
       count(*) FILTER (WHERE q.state = 'answered') AS answered
  FROM workspace.questions q
 WHERE q.run_id = 'fleet-change-record-3-30a0427e'
