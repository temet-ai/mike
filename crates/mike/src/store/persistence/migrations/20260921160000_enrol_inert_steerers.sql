-- Issue #320: three efficiency steerers were enabled with auto_enrol=false, so no session was ever
-- enrolled and none ever evaluated. A steerer evaluates a session only through an active
-- steering.enrolments row, and auto_enrol=true is what creates that row when a session's first
-- request arrives. memory-keeper has no dedicated code and can only be inert, so it is disabled.
--
-- Rows are matched by name: ids differ between databases. Each UPDATE only touches a row that
-- still carries the old value, so a re-run changes nothing and leaves updated_at alone.
-- Sessions that already exist are not enrolled retroactively; only sessions whose first request
-- arrives after this migration is applied pick up the new auto-enrolment.

UPDATE steering.steerers
   SET auto_enrol = true,
       updated_at = NOW()
 WHERE name IN ('ast-grep-over-grep', 'monitor-over-sleep', 'tee-then-slice-gates')
   AND deleted_at IS NULL
   AND auto_enrol = false;

UPDATE steering.steerers
   SET enabled = false,
       updated_at = NOW()
 WHERE name = 'memory-keeper'
   AND deleted_at IS NULL
   AND enabled = true;
