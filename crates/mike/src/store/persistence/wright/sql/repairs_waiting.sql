SELECT EXISTS (SELECT 1 FROM wright.repairable_findings WHERE NOT repair_live)
   AND NOT EXISTS (SELECT 1 FROM wright.followups WHERE state = 'handed_off')
