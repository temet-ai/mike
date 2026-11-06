UPDATE steering.steerers
   SET trigger_point = 'turn_end'
 WHERE name = 'false-done' AND kind = 'false_done'
