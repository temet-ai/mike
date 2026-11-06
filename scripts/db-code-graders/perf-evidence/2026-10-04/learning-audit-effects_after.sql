SELECT family,count(*) AS rows,max(window_end) AS latest
          FROM eval.rates GROUP BY family ORDER BY family