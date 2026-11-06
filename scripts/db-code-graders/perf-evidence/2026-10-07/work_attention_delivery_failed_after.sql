UPDATE agents.work_item_attention
   SET delivery_attempts = delivery_attempts + 1, last_delivery_attempt_at = now()
 WHERE id = 2293649
