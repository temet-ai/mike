-- Tighten the false-done steerer's stage-1 pattern gate to genuine
-- completion-claim phrases only (issue #116 AC1). The prior list also
-- matched command names (`cargo check`, `make check`, …) and discourse
-- markers ("in summary", "as expected", "here is the", "straightforward",
-- "robust", "comprehensive", "production-ready", "production ready", "the
-- report", "wrapped up") that appear in ordinary engineering prose and tool
-- summaries without claiming the task itself is done, drawing ~15% of all
-- end-turns in steered sessions into judgment on that basis alone.
UPDATE steering.steerers
   SET pattern = E' done\n done.\n done,\ntests pass\ntest pass\n verified\n complete\n completed\n fixed\n it works\nnow working\nsuccessfully\nthe issue is resolved\nissue resolved\nall tests'
 WHERE name = 'false-done';
