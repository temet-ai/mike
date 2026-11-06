ALTER TABLE launch.tree_checks ADD COLUMN state_after TEXT;

COMMENT ON COLUMN launch.tree_checks.state_after IS 'The state of the tree when the check finished. When it differs from `state` the tree changed while the check ran, so the result belongs to neither state and is not served to a later asker.';
