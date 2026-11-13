ALTER TABLE config.feature_toggles
    ADD COLUMN operator_set boolean NOT NULL DEFAULT false;

UPDATE config.feature_toggles
   SET operator_set = true;
