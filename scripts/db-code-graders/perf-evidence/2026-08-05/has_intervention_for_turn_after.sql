SELECT EXISTS(
    SELECT 1 FROM steering.interventions
     WHERE steerer_id = 6 AND operator_turn_id = 12285
) AS "exists!";
