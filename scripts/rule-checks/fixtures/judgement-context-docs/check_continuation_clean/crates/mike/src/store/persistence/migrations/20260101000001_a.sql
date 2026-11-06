CREATE TABLE memory.turn_verdicts (
    id bigint PRIMARY KEY,
    verdict text NOT NULL,
    CONSTRAINT turn_verdicts_shape
        CHECK (verdict IS NOT NULL
               AND (verdict <> 'x'
                    OR id > 0)),
    parent bigint
        REFERENCES memory.turn_verdicts(id)
);
