-- A confirmed cause outside the repository (kind external, config, leftover or unknown) names no
-- repository path. Only a confirmed code_defect must carry owning_part; the repair queue already
-- takes code_defect findings with a part only.
ALTER TABLE wright.findings
    DROP CONSTRAINT findings_established_names_cause_and_part,
    ADD CONSTRAINT findings_established_names_cause_and_part
        CHECK (NOT cause_established
               OR (confirmed_cause IS NOT NULL
                   AND (owning_part IS NOT NULL OR kind <> 'code_defect')));
